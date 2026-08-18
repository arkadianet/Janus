pub fn first_run() -> String {
    "Add a folder you already keep models in. Janus will not move these files.\n\
     \n\
     janus root add PATH\n\
     janus scan\n\
     janus list\n\
     janus doctor\n\
     \n\
     Same catalogue in a browser:\n\
     janus daemon\n\
     # open http://127.0.0.1:4321\n\
     \n\
     janus --help for every command.\n"
        .into()
}

pub fn help() -> String {
    "janus — local catalogue for model files you already own\n\
     \n\
     Build: cargo build --release   (binary: target/release/janus)\n\
     \n\
     Catalogue (offline):\n\
       root add PATH [--kind internal|nas|removable|fetch] [--name NAME] [--cold] [--accept-marker]\n\
       root ls|rm|probe|discover [--json]\n\
       scan [--quick] [ROOT]\n\
       status [--json]\n\
       list [--json]\n\
       search QUERY [--json]\n\
       show FAMILY [--json]\n\
       identify FILE [--name NAME] [--non-interactive]\n\
       merge SRC TARGET | --decline A B\n\
       verify FILE_OR_ID\n\
       dedup --plan [--json]\n\
       storage [--json]\n\
       cold mark|unmark ID\n\
       doctor [--json]\n\
       export PATH\n\
       import PATH\n\
     \n\
     Browse:\n\
       daemon [--api 127.0.0.1:4321]   # http://127.0.0.1:4321\n\
     \n\
     Radar / fetch (opt-in):\n\
       profile ls|show|set\n\
       monitor add|ls|rm\n\
       radar [FAMILY...] [--once]      # lists remote files; does not download\n\
       wanted [--open|--have-offline]\n\
       fetch ID [--force] [--file NAME] | status\n\
     \n\
     Other:\n\
       db\n\
       cases [FIXTURES_DIR]\n\
       have rel_path --root ID\n\
     \n\
     --accept-marker writes .janus-root only when the volume has no UUID/serial.\n\
     Discovery roots (Ollama / LM Studio / HF cache) are never written.\n"
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_prints_three_commands_and_doctor() {
        let t = first_run();
        assert!(t.contains("janus root add"));
        assert!(t.contains("janus scan"));
        assert!(t.contains("janus list"));
        assert!(t.contains("janus doctor"));
        assert!(t.contains("127.0.0.1:4321"));
        assert!(t.contains("will not move"));
    }

    #[test]
    fn help_lists_catalogue_and_radar() {
        let t = help();
        assert!(t.contains("dedup --plan"));
        assert!(t.contains("verify"));
        assert!(t.contains("--accept-marker"));
        assert!(t.contains("daemon"));
        assert!(t.contains("radar"));
    }
}
