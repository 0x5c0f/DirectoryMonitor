use dm_core::config::*;
use std::path::PathBuf;

#[test]
fn test_config_default_values() {
    let config = AppConfig::default();

    assert_eq!(config.server.bind, "127.0.0.1");
    assert_eq!(config.server.port, 8080);
    assert!(config.server.password.is_empty());
    assert!(config.watches.is_empty());
    assert!(config.database.enabled);
    assert_eq!(config.logging.level, "info");
}

#[test]
fn test_config_toml_roundtrip() {
    let mut config = AppConfig::default();
    config.server.port = 9090;
    config.server.password = "secret".to_string();
    config.watches.push(WatchConfig {
        path: PathBuf::from("/home/user/docs"),
        recursive: true,
        include: vec!["*.txt".to_string()],
        exclude: vec!["**/.git/**".to_string()],
        event_types: vec!["create".to_string(), "modify".to_string()],
        log_format: None,
        script: None,
        script_mode: "async".to_string(),
        script_events: vec![],
        email_recipients: vec![],
    });

    // Serialize to TOML
    let toml_str = toml::to_string(&config).unwrap();
    assert!(toml_str.contains("port = 9090"));
    assert!(toml_str.contains("password = \"secret\""));
    assert!(toml_str.contains("/home/user/docs"));

    // Deserialize back
    let config2: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(config2.server.port, 9090);
    assert_eq!(config2.server.password, "secret");
    assert_eq!(config2.watches.len(), 1);
    assert_eq!(config2.watches[0].path, PathBuf::from("/home/user/docs"));
    assert_eq!(config2.watches[0].include, vec!["*.txt"]);
    assert_eq!(config2.watches[0].exclude, vec!["**/.git/**"]);
}

#[test]
fn test_config_toml_roundtrip_with_defaults() {
    let config = AppConfig::default();
    let toml_str = toml::to_string(&config).unwrap();
    let config2: AppConfig = toml::from_str(&toml_str).unwrap();

    // All defaults should survive roundtrip
    assert_eq!(config.server.bind, config2.server.bind);
    assert_eq!(config.server.port, config2.server.port);
    assert_eq!(config.database.enabled, config2.database.enabled);
    assert_eq!(config.logging.level, config2.logging.level);
}

#[test]
fn test_config_save_and_load() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = PathBuf::from(tmp.path());

    let mut config = AppConfig::default();
    config.server.port = 3000;
    config.watches.push(WatchConfig {
        path: PathBuf::from("/tmp/watch"),
        recursive: false,
        include: vec![],
        exclude: vec!["*.tmp".to_string()],
        event_types: vec![],
        log_format: None,
        script: None,
        script_mode: "async".to_string(),
        script_events: vec![],
        email_recipients: vec![],
    });

    // Save
    config.save(&path).unwrap();

    // Load
    let config2 = AppConfig::load(&path).unwrap();
    assert_eq!(config2.server.port, 3000);
    assert_eq!(config2.watches.len(), 1);
    assert_eq!(config2.watches[0].path, PathBuf::from("/tmp/watch"));
    assert!(!config2.watches[0].recursive);
    assert_eq!(config2.watches[0].exclude, vec!["*.tmp"]);
}

#[test]
fn test_config_load_nonexistent_returns_error() {
    let config = AppConfig::load(&PathBuf::from("/nonexistent/path/config.toml"));
    // Should return error for nonexistent path
    assert!(config.is_err());
}

#[test]
fn test_watch_config_serialization() {
    let watch = WatchConfig {
        path: PathBuf::from("/home/user"),
        recursive: true,
        include: vec!["*.rs".to_string(), "*.toml".to_string()],
        exclude: vec!["**/target/**".to_string()],
        event_types: vec!["create".to_string(), "modify".to_string()],
        log_format: Some("%event% %file%".to_string()),
        script: Some("notify-send 'event'".to_string()),
        script_mode: "async".to_string(),
        script_events: vec!["create".to_string()],
        email_recipients: vec!["admin@example.com".to_string()],
    };

    let toml_str = toml::to_string(&watch).unwrap();
    let watch2: WatchConfig = toml::from_str(&toml_str).unwrap();

    assert_eq!(watch2.path, watch.path);
    assert_eq!(watch2.recursive, watch.recursive);
    assert_eq!(watch2.include, watch.include);
    assert_eq!(watch2.exclude, watch.exclude);
    assert_eq!(watch2.event_types, watch.event_types);
    assert_eq!(watch2.log_format, watch.log_format);
    assert_eq!(watch2.script, watch.script);
    assert_eq!(watch2.script_mode, watch.script_mode);
    assert_eq!(watch2.script_events, watch.script_events);
    assert_eq!(watch2.email_recipients, watch.email_recipients);
}
