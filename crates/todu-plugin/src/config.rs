use nu_plugin::EngineInterface;
use nu_protocol::Value;
use std::path::PathBuf;
use todu_db::UrgencyCoefficients;

pub struct Config {
    pub db_path: PathBuf,
    pub default_global: bool,
    pub urgency: UrgencyCoefficients,
}

impl Config {
    fn default_db_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".local/share/nu_plugin_todu/todu.db")
    }

    pub fn from_engine(engine: &EngineInterface) -> Self {
        let cfg = engine.get_plugin_config().ok().flatten();
        let get_cfg_val = |key: &str| {
            cfg.as_ref()
                .and_then(|config_val| config_val.as_record().ok())
                .and_then(|record| record.get(key))
                .cloned()
        };

        let db_path = get_cfg_val("db_path")
            .and_then(|val| val.as_str().ok().map(|path_str| path_str.to_string()))
            .map(|path_str| {
                if path_str.starts_with("~/") {
                    let home = std::env::var("HOME").unwrap_or_default();
                    PathBuf::from(format!("{home}{}", &path_str[1..]))
                } else {
                    PathBuf::from(path_str)
                }
            })
            .unwrap_or_else(Self::default_db_path);

        let default_global = get_cfg_val("default_global")
            .and_then(|val| val.as_bool().ok())
            .unwrap_or(false);

        let urgency = get_cfg_val("urgency")
            .map(|val| urgency_from_value(&val))
            .unwrap_or_default();

        Config {
            db_path,
            default_global,
            urgency,
        }
    }
}

fn as_number(val: &Value) -> Option<f64> {
    val.as_float()
        .ok()
        .or_else(|| val.as_int().ok().map(|i| i as f64))
}

/// Reads urgency coefficient overrides from a record such as
/// `{ due: 10.0, priority_high: 7, tag: { next: 20, someday: -4 } }`. Unknown keys and
/// non-numeric values are ignored so a typo never breaks listing todos.
fn urgency_from_value(val: &Value) -> UrgencyCoefficients {
    let mut coeffs = UrgencyCoefficients::default();
    let Ok(record) = val.as_record() else {
        return coeffs;
    };
    for (key, val) in record.iter() {
        if key == "tag" {
            if let Ok(tags) = val.as_record() {
                for (tag, val) in tags.iter() {
                    if let Some(n) = as_number(val) {
                        coeffs.set(&format!("tag.{tag}"), n);
                    }
                }
            }
        } else if let Some(n) = as_number(val) {
            coeffs.set(key, n);
        }
    }
    coeffs
}
