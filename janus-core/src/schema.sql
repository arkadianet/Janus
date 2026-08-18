-- Canonical Phase A–D schema. Spec only until janus-core exists.
-- Every connection: PRAGMA foreign_keys = ON;
-- schema_version in meta; changing family_key algorithm is a migration.

PRAGMA foreign_keys = ON;

CREATE TABLE meta (
  k TEXT PRIMARY KEY,
  v TEXT
);

INSERT INTO meta (k, v) VALUES ('schema_version', '1');
INSERT INTO meta (k, v) VALUES ('family_key_algo', '1');

CREATE TABLE storage_roots (
  id INTEGER PRIMARY KEY,
  name TEXT,
  path TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL,
  mode TEXT NOT NULL DEFAULT 'catalogue',
  mount_id TEXT,
  present INTEGER,
  last_present_check INTEGER,
  last_scan_at INTEGER,
  cold INTEGER NOT NULL DEFAULT 0,
  writable INTEGER NOT NULL DEFAULT 0,
  CHECK (
    (kind = 'fetch' AND mode = 'fetch' AND writable = 1) OR
    (kind != 'fetch' AND mode = 'catalogue' AND writable = 0)
  )
);

CREATE UNIQUE INDEX storage_roots_one_fetch
  ON storage_roots(kind) WHERE kind = 'fetch';

CREATE TABLE blobs (
  id INTEGER PRIMARY KEY,
  blake3 TEXT UNIQUE,
  sha256 TEXT,
  size INTEGER NOT NULL,
  refcount INTEGER, -- unused; live count is SELECT COUNT(*) FROM files WHERE blob_id=?
  xxhash64_partial TEXT
);

CREATE TABLE files (
  id INTEGER PRIMARY KEY,
  root_id INTEGER NOT NULL REFERENCES storage_roots ON DELETE CASCADE,
  rel_path TEXT NOT NULL,
  size INTEGER,
  mtime INTEGER,
  ctime INTEGER,
  dev INTEGER,
  ino INTEGER,
  change_gen TEXT,
  is_symlink INTEGER,
  symlink_target TEXT,
  blob_id INTEGER REFERENCES blobs,
  hash_state TEXT DEFAULT 'none',
  parse_state TEXT DEFAULT 'pending',
  parse_error TEXT,
  state TEXT NOT NULL DEFAULT 'present',
  UNIQUE (root_id, rel_path)
);

CREATE TABLE model_families (
  id INTEGER PRIMARY KEY,
  family_key TEXT NOT NULL UNIQUE,
  name TEXT,
  arch TEXT,
  params_total REAL,
  params_active REAL,
  context_len INTEGER,
  kind TEXT DEFAULT 'unknown'
);

CREATE TABLE family_aliases (
  family_id INTEGER REFERENCES model_families,
  alias TEXT,
  source TEXT,
  UNIQUE (alias)
);

CREATE TABLE declined_merges (
  family_a_key TEXT NOT NULL,
  family_b_key TEXT NOT NULL,
  algo_version TEXT NOT NULL,
  declined_at INTEGER,
  PRIMARY KEY (family_a_key, family_b_key, algo_version),
  CHECK (family_a_key < family_b_key)
);

CREATE TABLE model_revisions (
  id INTEGER PRIMARY KEY,
  family_id INTEGER NOT NULL REFERENCES model_families,
  rev_kind TEXT NOT NULL,
  rev_label TEXT NOT NULL,
  source_hint TEXT,
  UNIQUE (family_id, rev_kind, rev_label)
);

CREATE TABLE model_variants (
  id INTEGER PRIMARY KEY,
  family_id INTEGER NOT NULL REFERENCES model_families,
  revision_id INTEGER NOT NULL REFERENCES model_revisions,
  quant TEXT NOT NULL DEFAULT 'unknown',
  quant_raw TEXT,
  format TEXT NOT NULL DEFAULT 'unknown',
  subflavour TEXT NOT NULL DEFAULT 'unknown',
  publisher TEXT NOT NULL DEFAULT 'unknown',
  UNIQUE (family_id, revision_id, format, quant, subflavour, publisher)
);

CREATE TABLE file_roles (
  file_id INTEGER REFERENCES files ON DELETE CASCADE,
  variant_id INTEGER REFERENCES model_variants ON DELETE SET NULL,
  family_id INTEGER REFERENCES model_families ON DELETE SET NULL,
  role TEXT,
  PRIMARY KEY (file_id)
);

CREATE TABLE evidence (
  id INTEGER PRIMARY KEY,
  subject_type TEXT,
  subject_id INTEGER,
  field TEXT,
  value TEXT,
  level TEXT,
  source TEXT,
  recorded_at INTEGER
);

CREATE TABLE provenance_entries (
  id INTEGER PRIMARY KEY,
  subject_type TEXT,
  subject_id INTEGER,
  event TEXT,
  source_kind TEXT,
  url TEXT,
  repo TEXT,
  author TEXT,
  licence TEXT,
  revision TEXT,
  at INTEGER,
  checksum TEXT
);

CREATE TABLE enrichments (
  id INTEGER PRIMARY KEY,
  subject_type TEXT,
  subject_id INTEGER,
  provider TEXT,
  payload_json TEXT,
  fetched_at INTEGER,
  etag TEXT
);

CREATE TABLE quality_profiles (
  id INTEGER PRIMARY KEY,
  name TEXT UNIQUE,
  spec_json TEXT
);

CREATE TABLE monitors (
  id INTEGER PRIMARY KEY,
  family_id INTEGER NOT NULL REFERENCES model_families,
  variant_id INTEGER REFERENCES model_variants,
  profile_id INTEGER NOT NULL REFERENCES quality_profiles,
  enabled INTEGER DEFAULT 1
);

CREATE TABLE wanted_items (
  id INTEGER PRIMARY KEY,
  monitor_id INTEGER REFERENCES monitors,
  remote_key TEXT NOT NULL UNIQUE,
  provider TEXT NOT NULL,
  repo TEXT NOT NULL,
  revision TEXT NOT NULL,
  filename TEXT NOT NULL,
  size INTEGER,
  sha256 TEXT,
  status TEXT,
  local_blob_id INTEGER REFERENCES blobs,
  local_root_id INTEGER REFERENCES storage_roots
);

CREATE TABLE fetch_tasks (
  id INTEGER PRIMARY KEY,
  wanted_id INTEGER NOT NULL REFERENCES wanted_items,
  dest_root_id INTEGER NOT NULL REFERENCES storage_roots,
  dest_rel_path TEXT NOT NULL,
  bytes_done INTEGER,
  bytes_total INTEGER,
  state TEXT,
  error TEXT
);

CREATE UNIQUE INDEX fetch_tasks_one_active
  ON fetch_tasks(wanted_id) WHERE state IN ('queued', 'running', 'paused');

CREATE TABLE tags (
  id INTEGER PRIMARY KEY,
  name TEXT UNIQUE
);

CREATE TABLE tagmap (
  tag_id INTEGER,
  entity_type TEXT,
  entity_id INTEGER,
  PRIMARY KEY (tag_id, entity_type, entity_id)
);

CREATE TABLE jobs (
  id INTEGER PRIMARY KEY,
  kind TEXT,
  state TEXT,
  progress REAL,
  started INTEGER,
  finished INTEGER,
  error_json TEXT
);

CREATE TABLE scan_runs (
  id INTEGER PRIMARY KEY,
  root_id INTEGER,
  started INTEGER,
  finished INTEGER,
  files_new INTEGER,
  files_changed INTEGER,
  files_gone INTEGER,
  ok INTEGER
);

CREATE INDEX files_root_id ON files(root_id);
CREATE INDEX files_blob_id ON files(blob_id);
CREATE INDEX blobs_sha256 ON blobs(sha256);
CREATE INDEX families_kind ON model_families(kind);
CREATE INDEX families_params ON model_families(params_total);
CREATE INDEX variants_family ON model_variants(family_id);
CREATE INDEX evidence_subject ON evidence(subject_type, subject_id, field);
CREATE INDEX file_roles_variant ON file_roles(variant_id);
CREATE INDEX file_roles_family ON file_roles(family_id);

-- App (not SQL):
-- * at most one fetch root (index above)
-- * fetch/catalogue paths must not nest
-- * monitor.variant_id IS NULL OR variant.family_id = monitor.family_id
-- * fetch requires wanted_items.sha256 IS NOT NULL
-- * presence is inherited from storage_roots.present
