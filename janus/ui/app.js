const main = document.getElementById("main");
const searchBtn = document.getElementById("search-btn");

function api(path, opts) {
  return fetch(path, opts).then(async (r) => {
    const text = await r.text();
    let body = null;
    try { body = text ? JSON.parse(text) : null; } catch { body = { message: text }; }
    if (!r.ok) throw Object.assign(new Error((body && (body.message || body.code)) || r.statusText), { code: body && body.code, status: r.status });
    return body;
  });
}

function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function human(n) {
  const u = ["B", "K", "M", "G", "T", "P"];
  let v = Number(n) || 0, i = 0;
  if (v <= 0) return "0B";
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(1)}${u[i]}`;
}

function fact(value, level, empty) {
  const v = value == null || value === "" ? (empty || "—") : String(value);
  const lvl = level || "detected";
  const tilde = lvl === "inferred" ? `<span class="tilde" title="inferred">~</span>` : "";
  return `<span class="fact">${esc(v)}${tilde}<span class="lvl ${esc(lvl)}">${esc(lvl)}</span></span>`;
}

function setNav(path) {
  document.querySelectorAll("nav a").forEach((a) => {
    const href = a.getAttribute("href");
    a.setAttribute("aria-current", (href === "/" ? path === "/" : path.startsWith(href)) ? "page" : null);
  });
}

function route() {
  const path = location.pathname;
  setNav(path);
  if (path === "/") return renderHome();
  if (path === "/library") return renderLibrary();
  if (path.startsWith("/model/")) return renderModel(path.slice(7));
  if (path === "/unknown") return renderUnknown();
  if (path === "/storage") return renderStorage();
  if (path === "/search") return renderSearch(new URLSearchParams(location.search).get("q") || "");
  if (path === "/wanted") return renderWanted();
  if (path === "/settings") return renderSettings();
  return renderHome();
}

function go(href) {
  history.pushState({}, "", href);
  route();
}

document.body.addEventListener("click", (e) => {
  const a = e.target.closest("a[data-link]");
  if (!a) return;
  e.preventDefault();
  go(a.getAttribute("href"));
});
window.addEventListener("popstate", route);
searchBtn.addEventListener("click", () => go("/search"));
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    go("/search");
    const box = document.getElementById("q");
    if (box) box.focus();
  }
});

async function renderHome() {
  main.innerHTML = `<h1>Home</h1><p class="muted">Loading…</p>`;
  const [home, doctor] = await Promise.all([api("/api/v1/home"), api("/api/v1/doctor")]);
  const c = home.counts;
  const empty = c.roots === 0;
  if (empty) {
    main.innerHTML = `
      <section class="empty">
        <h1>Home</h1>
        <p class="lead">Add a folder you already keep models in.</p>
        <p>Janus will not move these files. No account, no Hugging Face token.</p>
        ${rootForm()}
      </section>`;
    bindRootForm();
    return;
  }
  const inferred = c.families_inferred || 0;
  const knownish = Math.max(0, (c.families || 0) - inferred);
  const offline = (home.roots || []).filter((r) => !r.present);
  const findings = (doctor.findings || []).map((f) => `<li class="warn">${esc(f.code)} — ${esc(f.message)} (${f.count})</li>`).join("");
  main.innerHTML = `
    <h1>Home</h1>
    <div class="grid">
      <div class="card"><b>${c.families}</b><span>families · ${knownish} known/manual · ${inferred} inferred</span></div>
      <div class="card"><b>${c.files}</b><span>files · ${human(c.bytes)}</span></div>
      <div class="card"><b>${human(c.reclaimable)}</b><span>reclaimable (report only)</span></div>
      <div class="card"><b>${c.unknown_files}</b><span>unknown${c.unknown_files ? ` · <a href="/unknown" data-link>inbox</a>` : ""}</span></div>
      ${c.wanted_open ? `<div class="card"><b>${c.wanted_open}</b><span>open wanted · <a href="/wanted" data-link>Wanted</a></span></div>` : ""}
    </div>
    <p class="muted">Inferred names are never counted as known. ~ means guessed from a filename.</p>
    ${rootForm()}
    <div class="row"><button class="act" id="scan">Scan present roots</button>
      <button class="act" id="scan-quick">Quick scan</button>
      <span class="muted">Quick: grouping yes, duplicates/ownership no until a full hash.</span></div>
    <h2>Roots</h2>
    <table><thead><tr><th>Name</th><th>Kind</th><th>Present</th><th>Path</th></tr></thead>
    <tbody>${(home.roots || []).map((r) => `<tr class="${r.present ? "" : "offline"}"><td>${esc(r.name)}${r.cold ? " (cold)" : ""}</td><td>${esc(r.kind)}</td><td>${r.present ? "yes" : "no · last seen " + (r.last_present_check || "—")}</td><td>${esc(r.path)}</td></tr>`).join("")}</tbody></table>
    ${offline.length ? `<p class="offline">Offline roots stay in the catalogue. Reveal-in-folder needs the drive present.</p>` : ""}
    <h2>Recently seen</h2>
    ${fileTable(home.recent || [])}
    ${findings ? `<h2>Doctor</h2><ul>${findings}</ul>` : ""}`;
  bindRootForm();
  document.getElementById("scan").onclick = () => runScan(false);
  document.getElementById("scan-quick").onclick = () => runScan(true);
}

function rootForm() {
  return `<form id="add-root" class="row">
    <label>Path <input name="path" type="text" required placeholder="C:\\models or /home/you/models"></label>
    <label>Name <input name="name" type="text" placeholder="optional"></label>
    <label>Kind <select name="kind"><option value="internal">catalogue</option><option value="removable">removable</option><option value="nas">nas</option><option value="fetch">fetch</option></select></label>
    <label><input type="checkbox" name="accept_marker"> Write .janus-root if this volume has no UUID</label>
    <button class="act" type="submit">Add root</button>
  </form>`;
}

function bindRootForm() {
  const form = document.getElementById("add-root");
  if (!form) return;
  form.onsubmit = async (e) => {
    e.preventDefault();
    const fd = new FormData(form);
    try {
      await api("/api/v1/roots", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ path: fd.get("path"), name: fd.get("name") || undefined, kind: fd.get("kind"), accept_marker: fd.get("accept_marker") === "on" }),
      });
      route();
    } catch (err) { main.insertAdjacentHTML("afterbegin", `<p class="warn">${esc(err.message)}</p>`); }
  };
}

async function runScan(quick) {
  try {
    const job = await api("/api/v1/scan", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ quick }),
    });
    main.insertAdjacentHTML("afterbegin", `<p>Scan job ${job.job_id} finished. Reloading…</p>`);
    route();
  } catch (err) { main.insertAdjacentHTML("afterbegin", `<p class="warn">${esc(err.message)}</p>`); }
}

async function renderLibrary() {
  const q = new URLSearchParams(location.search);
  main.innerHTML = `<h1>Library</h1><p class="muted">Loading…</p>`;
  const data = await api("/api/v1/models?" + q.toString());
  const rows = (data.families || []).map((f) => {
    const roots = (f.roots || []).map((r) => r.present ? esc(r.name) : `[${esc(r.name)}]`).join(", ") || "—";
    return `<tr>
      <td><a href="/model/${f.id}" data-link>${fact(f.name.value || f.family_key.split("|")[0], f.name.level)}</a></td>
      <td>${fact(f.kind.value, f.kind.level)}</td>
      <td>${f.params_total != null ? esc(f.params_total) + "B" : "—"}</td>
      <td><div class="ladder">${(f.quants || "").split(",").filter(Boolean).map((x) => `<span class="chip">${esc(x)}</span>`).join("") || "—"}</div></td>
      <td>${human(f.bytes)}</td>
      <td>${roots}</td>
    </tr>`;
  }).join("");
  main.innerHTML = `
    <h1>Library</h1>
    <p>${data.counts.families} families · ${data.counts.families_inferred} name-inferred</p>
    <form class="row" id="filters">
      <input name="q" type="search" placeholder="filter name" value="${esc(q.get("q") || "")}">
      <input name="kind" type="text" placeholder="kind" value="${esc(q.get("kind") || "")}">
      <label><input type="checkbox" name="offline" ${q.get("offline") ? "checked" : ""}> offline</label>
      <label><input type="checkbox" name="dups" ${q.get("dups") ? "checked" : ""}> duplicates</label>
      <button class="act" type="submit">Apply</button>
    </form>
    <table><thead><tr><th>Family</th><th>Kind</th><th>Params</th><th>Variants</th><th>Size</th><th>Roots</th></tr></thead>
    <tbody>${rows || `<tr><td colspan="6">No families. Add a root and scan.</td></tr>`}</tbody></table>`;
  document.getElementById("filters").onsubmit = (e) => {
    e.preventDefault();
    const fd = new FormData(e.target);
    const n = new URLSearchParams();
    if (fd.get("q")) n.set("q", fd.get("q"));
    if (fd.get("kind")) n.set("kind", fd.get("kind"));
    if (fd.get("offline")) n.set("offline", "1");
    if (fd.get("dups")) n.set("dups", "1");
    go("/library" + (n.toString() ? "?" + n : ""));
  };
}

async function renderModel(id) {
  main.innerHTML = `<h1>Model</h1><p class="muted">Loading…</p>`;
  let data;
  try { data = await api("/api/v1/models/" + encodeURIComponent(id)); }
  catch (err) { main.innerHTML = `<h1>Model</h1><p class="warn">${esc(err.message)}</p>`; return; }
  const f = data.family;
  const variants = (data.variants || []).map((v) => `<tr>
    <td>${fact(v.quant.value, v.quant.level)}</td>
    <td>${fact(v.format.value, v.format.level)}</td>
    <td>${fact(v.subflavour.value, v.subflavour.level)}</td>
    <td>${fact(v.publisher.value, v.publisher.level)}</td>
    <td>${human(v.bytes)}</td>
    <td class="${v.present ? "" : "offline"}">${v.present ? esc(v.root) : "[" + esc(v.root) + "]"}</td>
  </tr>`).join("");
  const ev = (data.evidence || []).map((e) => `<tr><td>${esc(e.field)}</td><td>${esc(e.value)}</td><td>${fact(e.level, e.level)}</td><td>${esc(e.source)}</td></tr>`).join("");
  const prov = (data.provenance || []).map((p) => `<li>${esc(p.event)} ${esc(p.source_kind)} ${esc(p.repo || "")} ${p.at || ""}</li>`).join("");
  main.innerHTML = `
    <h1>${fact(f.name.value || f.family_key, f.name.level)}</h1>
    <p class="muted">key=${esc(f.family_key)}</p>
    <p>Kind ${fact(f.kind.value, f.kind.level)}</p>
    <h2>Variants</h2>
    <table><thead><tr><th>Quant</th><th>Format</th><th>Subflavour</th><th>Publisher</th><th>Size</th><th>Root</th></tr></thead>
    <tbody>${variants || `<tr><td colspan="6">None</td></tr>`}</tbody></table>
    <h2>Files</h2>
    ${fileTable(data.files || [])}
    <h2>Evidence</h2>
    <table><thead><tr><th>Field</th><th>Value</th><th>Level</th><th>Source</th></tr></thead>
    <tbody>${ev || `<tr><td colspan="4">None</td></tr>`}</tbody></table>
    <h2>Provenance</h2>
    <ul>${prov || `<li class="muted">None yet</li>`}</ul>
    <p class="row"><button class="act" id="radar-fam">Radar this family</button>
      <button class="act" id="verify-first">Verify first file</button>
      <span class="muted">Sends repo id / revision / remote file names to Hugging Face. Weights stay here.</span></p>
    <h2>Identify / merge</h2>
    <form class="row" id="merge"><input name="src" placeholder="source family"><input name="target" placeholder="target family"><button class="act">Merge</button></form>
    <form class="row" id="decline"><input name="a" placeholder="family A"><input name="b" placeholder="family B"><button class="act">Decline merge</button></form>`;
  document.getElementById("verify-first").onclick = async () => {
    const first = (data.files || [])[0];
    if (!first) { alert("No files on this family."); return; }
    try {
      const out = await api("/api/v1/verify", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ target: String(first.id), full: true }) });
      alert("blake3 " + (out.blake3 || "").slice(0, 16) + "…");
    } catch (err) { alert(err.message); }
  };
  document.getElementById("radar-fam").onclick = async () => {
    try {
      await api("/api/v1/monitors", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ family_id: f.id, profile: "daily-llm" }) });
      await api("/api/v1/radar", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ opt_in: true, families: [f.family_key] }) });
      go("/wanted");
    } catch (err) { alert(err.message); }
  };
  document.getElementById("merge").onsubmit = async (e) => {
    e.preventDefault();
    const fd = new FormData(e.target);
    try { await api("/api/v1/merge", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ src: fd.get("src"), target: fd.get("target") }) }); route(); }
    catch (err) { alert(err.message); }
  };
  document.getElementById("decline").onsubmit = async (e) => {
    e.preventDefault();
    const fd = new FormData(e.target);
    try { await api("/api/v1/merge", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ decline: true, a: fd.get("a"), b: fd.get("b") }) }); route(); }
    catch (err) { alert(err.message); }
  };
}

async function renderUnknown() {
  main.innerHTML = `<h1>Unknown</h1><p class="muted">Loading…</p>`;
  const files = await api("/api/v1/files?unknown=1");
  const rows = (files || []).map((f) => `<tr>
    <td>${esc(f.root)}/${esc(f.rel_path)}</td><td>${human(f.size)}</td>
    <td>${esc(f.parse_state)}</td><td>${esc(f.hash_state)}</td>
    <td><form data-id="${f.id}" class="row idform"><input name="name" placeholder="type a name"><button class="act">Identify</button></form></td>
  </tr>`).join("");
  main.innerHTML = `
    <h1>Unknown</h1>
    <p>Files that parsed but have no family yet. Identify writes a <span class="lvl manual">manual</span> name. Searchable after naming.</p>
    <table><thead><tr><th>Path</th><th>Size</th><th>Parse</th><th>Hash</th><th>Name</th></tr></thead>
    <tbody>${rows || `<tr><td colspan="5">Inbox empty.</td></tr>`}</tbody></table>`;
  main.querySelectorAll(".idform").forEach((form) => {
    form.onsubmit = async (e) => {
      e.preventDefault();
      try {
        await api("/api/v1/identify", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ file_id: Number(form.dataset.id), name: new FormData(form).get("name") }),
        });
        route();
      } catch (err) { alert(err.message); }
    };
  });
}

async function renderStorage() {
  main.innerHTML = `<h1>Storage</h1><p class="muted">Loading…</p>`;
  const [stg, dups] = await Promise.all([api("/api/v1/storage"), api("/api/v1/dups")]);
  const max = Math.max(1, ...(stg.roots || []).map((r) => r.bytes));
  const bars = (stg.roots || []).map((r) => `
    <div class="card">
      <b>${esc(r.name)}</b>
      <span>${r.present ? "present" : "offline"} · ${r.files} files · ${human(r.bytes)} · reclaimable ${r.present ? human(r.reclaimable) : "0 (offline; not in apply)"}</span>
      <div class="bar" title="${human(r.bytes)}"><i style="width:${Math.round((r.bytes / max) * 100)}%"></i></div>
    </div>`).join("");
  const groups = (dups.groups || []).map((g) => `<tr><td>${esc(g.blake3.slice(0, 12))}…</td><td>${human(g.size)}</td><td>${g.copies}</td><td>${g.allocations}</td><td>${human(g.reclaimable)}</td><td>${esc((g.paths || []).join(", "))}</td></tr>`).join("");
  main.innerHTML = `
    <h1>Storage</h1>
    <p>Reclaimable uses unique (mount_id, dev, ino), not size × (N−1). No delete button.</p>
    <p>Present reclaimable: <b>${human(stg.reclaimable)}</b></p>
    ${bars || `<p>No roots.</p>`}
    <h2>Duplicates</h2>
    <table><thead><tr><th>Blob</th><th>Size</th><th>Copies</th><th>Inodes</th><th>Reclaimable</th><th>Paths</th></tr></thead>
    <tbody>${groups || `<tr><td colspan="6">No verified duplicate groups.</td></tr>`}</tbody></table>`;
}

async function renderSearch(initial) {
  main.innerHTML = `
    <h1>Search</h1>
    <p class="muted">Same engine as <code>janus search</code>. Chips: <code>quant:</code> <code>params:</code> <code>offline</code> <code>wanted</code> <code>have-bytes</code>. <kbd>Ctrl</kbd>+<kbd>K</kbd></p>
    <form class="row" id="sf"><input id="q" name="q" type="search" value="${esc(initial)}" placeholder="name, path, hash" autofocus><button class="act">Search</button></form>
    <div id="hits"></div>`;
  const box = document.getElementById("q");
  box.focus();
  document.getElementById("sf").onsubmit = (e) => {
    e.preventDefault();
    go("/search?q=" + encodeURIComponent(box.value));
  };
  if (!initial.trim()) {
    document.getElementById("hits").innerHTML = `<p class="muted">Type a query.</p>`;
    return;
  }
  const chips = parseChips(initial);
  const reqs = [
    api("/api/v1/search?q=" + encodeURIComponent(chips.text || initial)),
    api("/api/v1/models?" + chips.params.toString()),
  ];
  if (chips.params.get("wanted")) reqs.push(api("/api/v1/wanted?open=1"));
  const [hits, models, wanted] = await Promise.all(reqs);
  const fam = (models.families || []).map((f) => `<tr><td>family</td><td><a href="/model/${f.id}" data-link>${esc(f.name.value || f.family_key)}</a></td><td>${esc(f.family_key)}</td></tr>`).join("");
  const extra = (hits.hits || []).filter((h) => h.kind !== "family").map((h) => `<tr><td>${esc(h.kind)}</td><td>${esc(h.name)}</td><td class="${h.present ? "" : "offline"}">${esc(h.path || h.key || "")}</td></tr>`).join("");
  const want = ((wanted && wanted.items) || []).map((w) => `<tr><td>wanted</td><td>${esc(w.filename)}</td><td>${esc(w.status)} ${esc(w.note)}</td></tr>`).join("");
  document.getElementById("hits").innerHTML = `<table><thead><tr><th>Kind</th><th>Name</th><th>Key / path</th></tr></thead><tbody>${fam}${extra}${want}</tbody></table>`;
}

function parseChips(q) {
  const params = new URLSearchParams();
  let text = q;
  if (/\boffline\b/.test(q)) { params.set("offline", "1"); text = text.replace(/\boffline\b/, "").trim(); }
  if (/\bwanted\b/.test(q)) { params.set("wanted", "1"); text = text.replace(/\bwanted\b/, "").trim(); }
  const quant = q.match(/quant:(\S+)/);
  if (quant) { params.set("q", quant[1]); text = text.replace(quant[0], "").trim(); }
  const paramsChip = q.match(/params:(\S+)/);
  if (paramsChip) { params.set("q", (params.get("q") || "") + " " + paramsChip[1]); text = text.replace(paramsChip[0], "").trim(); }
  if (text && !params.get("q")) params.set("q", text);
  return { text, params };
}

async function renderWanted() {
  main.innerHTML = `<h1>Wanted</h1><p class="muted">Loading…</p>`;
  const [data, monitors, profiles] = await Promise.all([
    api("/api/v1/wanted"),
    api("/api/v1/monitors"),
    api("/api/v1/profiles"),
  ]);
  const items = data.items || [];
  const open = items.filter((w) => w.status === "open");
  const haveOff = items.filter((w) => w.status === "skipped_have_bytes" && !w.local_present);
  const fetched = items.filter((w) => w.status === "fetched");
  const rows = items.map((w) => {
    const haveOffRow = w.status === "skipped_have_bytes" && !w.local_present;
    const label = haveOffRow ? "have-offline" : w.status;
    return `<tr class="${haveOffRow ? "offline" : ""}">
      <td>${w.id}</td>
      <td>${w.family_id ? `<a href="/model/${w.family_id}" data-link>${esc(w.family)}</a>` : esc(w.family)}</td>
      <td>${esc(w.revision)}</td>
      <td>${esc(w.filename)}</td>
      <td><span class="chip">${esc(label)}</span></td>
      <td>${esc(w.note || (haveOffRow ? "owned on an offline root — not missing" : ""))}</td>
      <td>${w.status === "open" && w.sha256 ? `<button class="act" data-fetch="${w.id}">Fetch</button>` : w.status === "open" ? "no digest" : ""}</td>
    </tr>`;
  }).join("");
  const mon = (monitors || []).map((m) => `<tr><td>${m.id}</td><td>${esc(m.family)}</td><td>${esc(m.profile)}</td><td>${m.enabled ? "yes" : "no"}</td></tr>`).join("");
  main.innerHTML = `
    <h1>Wanted</h1>
    <p>Offline-owns-it is not missing. Radar lists files; it does not download.</p>
    <p class="warn">${esc(data.privacy_notice)}</p>
    <div class="row">
      <button class="act" id="sweep">Sweep monitored families</button>
      <span class="muted">${open.length} open · ${haveOff.length} have-offline · ${fetched.length} fetched · ${(profiles || []).length} profiles</span>
    </div>
    <h2>Monitors</h2>
    <table><thead><tr><th>ID</th><th>Family</th><th>Profile</th><th>On</th></tr></thead>
    <tbody>${mon || `<tr><td colspan="4">None. Open a model and use Radar this family.</td></tr>`}</tbody></table>
    <h2>Listing</h2>
    <table><thead><tr><th>ID</th><th>Family</th><th>Rev</th><th>File</th><th>Status</th><th>Note</th><th></th></tr></thead>
    <tbody>${rows || `<tr><td colspan="7">Nothing wanted yet. Sweep is opt-in.</td></tr>`}</tbody></table>`;
  document.getElementById("sweep").onclick = async () => {
    if (!confirm(data.privacy_notice + "\n\nRun sweep?")) return;
    try {
      await api("/api/v1/radar", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ opt_in: true }) });
      route();
    } catch (err) { main.insertAdjacentHTML("afterbegin", `<p class="warn">${esc(err.message)}</p>`); }
  };
  main.querySelectorAll("[data-fetch]").forEach((btn) => {
    btn.onclick = async () => {
      if (!confirm("Fetch into the fetch root only. Verified-owned blobs need Force.")) return;
      try {
        await api("/api/v1/fetch", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ wanted_id: Number(btn.dataset.fetch) }),
        });
        route();
      } catch (err) { alert(err.message); }
    };
  });
}

async function renderSettings() {
  main.innerHTML = `<h1>Settings</h1><p class="muted">Loading…</p>`;
  const [home, doctor] = await Promise.all([api("/api/v1/home"), api("/api/v1/doctor")]);
  const findings = (doctor.findings || []).map((f) => `<li class="warn">${esc(f.code)} — ${esc(f.message)}</li>`).join("");
  const rows = (home.roots || []).map((r) => `<tr class="${r.present ? "" : "offline"}">
    <td>${esc(r.name)}</td><td>${esc(r.kind)}</td><td>${r.present ? "yes" : "no"}</td>
    <td>${r.cold ? "yes" : "no"}</td><td>${esc(r.mount_id || "—")}</td><td>${esc(r.path)}</td>
    <td class="row">
      <button class="act" data-probe="${r.id}">Probe</button>
      <button class="act" data-cold="${r.id}" data-on="${r.cold ? "0" : "1"}">${r.cold ? "Unmark cold" : "Mark cold"}</button>
      <button class="act" data-rm="${r.id}">Remove</button>
    </td>
  </tr>`).join("");
  main.innerHTML = `
    <h1>Settings</h1>
    <p>Catalogue is default. Janus will not move these files. Discovery roots stay read-only.</p>
    ${rootForm()}
    <div class="row">
      <button class="act" id="discover">Discover Ollama / LM Studio / HF cache</button>
      <button class="act" id="do-export">Export decisions</button>
    </div>
    <h2>Roots</h2>
    <table><thead><tr><th>Name</th><th>Kind</th><th>Present</th><th>Cold</th><th>mount_id</th><th>Path</th><th></th></tr></thead>
    <tbody>${rows || `<tr><td colspan="7">None yet.</td></tr>`}</tbody></table>
    <h2>Import</h2>
    <form class="row" id="imp"><textarea name="body" rows="6" placeholder='{"format":"janus.export",...}'></textarea><button class="act">Import</button></form>
    ${findings ? `<h2>Doctor</h2><ul>${findings}</ul>` : ""}`;
  bindRootForm();
  document.getElementById("discover").onclick = async () => {
    try { await api("/api/v1/roots/discover", { method: "POST" }); route(); }
    catch (err) { alert(err.message); }
  };
  document.getElementById("do-export").onclick = async () => {
    try {
      const v = await api("/api/v1/export");
      const blob = new Blob([JSON.stringify(v, null, 2)], { type: "application/json" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = "janus-export.json";
      a.click();
    } catch (err) { alert(err.message); }
  };
  document.getElementById("imp").onsubmit = async (e) => {
    e.preventDefault();
    try {
      await api("/api/v1/import", { method: "POST", headers: { "content-type": "application/json" }, body: new FormData(e.target).get("body") });
      route();
    } catch (err) { alert(err.message); }
  };
  main.querySelectorAll("[data-probe]").forEach((btn) => {
    btn.onclick = async () => {
      try { await api("/api/v1/roots/" + btn.dataset.probe + "/probe", { method: "POST" }); route(); }
      catch (err) { alert(err.message); }
    };
  });
  main.querySelectorAll("[data-cold]").forEach((btn) => {
    btn.onclick = async () => {
      try {
        await api("/api/v1/roots/" + btn.dataset.cold + "/cold", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ cold: btn.dataset.on === "1" }),
        });
        route();
      } catch (err) { alert(err.message); }
    };
  });
  main.querySelectorAll("[data-rm]").forEach((btn) => {
    btn.onclick = async () => {
      if (!confirm("Remove this root from the catalogue? Files on disk stay.")) return;
      try { await api("/api/v1/roots/" + btn.dataset.rm, { method: "DELETE" }); route(); }
      catch (err) { alert(err.message); }
    };
  });
}

function fileTable(files) {
  if (!files.length) return `<p class="muted">No files.</p>`;
  return `<table><thead><tr><th>Path</th><th>Size</th><th>State</th><th>Hash</th><th>Parse</th></tr></thead><tbody>
    ${files.map((f) => `<tr class="${f.present ? "" : "offline"}"><td>${esc(f.root)}/${esc(f.rel_path)}</td><td>${human(f.size)}</td><td>${esc(f.state)}</td><td>${esc(f.hash_state)}</td><td>${esc(f.parse_state)}</td></tr>`).join("")}
  </tbody></table>`;
}

route();
