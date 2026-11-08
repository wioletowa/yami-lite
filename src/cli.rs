// Командная строка для тех, кто любит конфиги: yami-lite config ….
// Работающий шелл замечает правку файла сам (перечитывает по времени изменения).

use crate::config::{settings_path, Settings};
use serde_json::Value;

const HELP: &str = "yami-lite config [list | get KEY | set KEY VALUE | reset [KEY] | check | path]
  list            every key with its value; * marks a value changed from the default
  get KEY         one value as JSON
  set KEY VALUE   VALUE is JSON (true, 12, [\"a\"], {…}) or else a plain string
  reset [KEY]     back to the default: one key, or the whole file (a .bak copy is kept)
  check           keys with a bad value (ignored, default used) and unknown keys
  path            where the file is
Keys are camelCase, as in ~/.config/yami-lite/settings.json.";

pub fn file_json() -> serde_json::Map<String, Value> {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

fn defaults() -> serde_json::Map<String, Value> {
    serde_json::to_value(Settings::default()).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// f32 настроек через Value выглядят как 1.2000000476837158: показываем как 1.2
fn tidy(v: &Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => n.as_f64().and_then(|x| format!("{}", x as f32).parse::<f64>().ok()).and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(v.clone()),
        Value::Array(a) => Value::Array(a.iter().map(tidy).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), tidy(x))).collect()),
        _ => v.clone(),
    }
}

fn save(s: &Settings) -> bool {
    crate::config::write_atomic(&settings_path(), &s.to_json()).is_ok()
}

pub fn config(args: &[String]) -> i32 {
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match a.as_slice() {
        ["path"] => println!("{}", settings_path().display()),
        [] | ["list"] => {
            let cur = serde_json::to_value(Settings::load()).unwrap_or_default();
            let def = defaults();
            println!("# {}", settings_path().display());
            for (k, d) in &def {
                let v = &cur[k];
                println!("{}{k} = {}", if v != d { "* " } else { "  " }, serde_json::to_string(&tidy(v)).unwrap_or_default());
            }
        }
        ["get", k] => {
            let cur = serde_json::to_value(Settings::load()).unwrap_or_default();
            match cur.get(*k) {
                Some(v) => println!("{}", serde_json::to_string_pretty(&tidy(v)).unwrap_or_default()),
                None => {
                    eprintln!("no such key: {k} (yami-lite config list)");
                    return 2;
                }
            }
        }
        ["set", k, v] => {
            if !defaults().contains_key(*k) {
                eprintln!("no such key: {k} (yami-lite config list)");
                return 2;
            }
            let val: Value = serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_string()));
            let mut cur = serde_json::to_value(Settings::load()).unwrap_or_default();
            cur[*k] = val;
            match serde_json::from_value::<Settings>(cur) {
                Ok(s) => {
                    if !save(&s) {
                        eprintln!("cannot write {}", settings_path().display());
                        return 1;
                    }
                }
                Err(e) => {
                    eprintln!("bad value for {k}: {e}");
                    return 2;
                }
            }
        }
        ["reset"] => {
            let p = settings_path();
            let _ = std::fs::copy(&p, p.with_extension("json.bak"));
            if !save(&Settings::default()) {
                return 1;
            }
            println!("defaults written; the old file is {}", p.with_extension("json.bak").display());
        }
        ["reset", k] => {
            let def = defaults();
            let Some(d) = def.get(*k) else {
                eprintln!("no such key: {k}");
                return 2;
            };
            let mut cur = serde_json::to_value(Settings::load()).unwrap_or_default();
            cur[*k] = d.clone();
            if !serde_json::from_value::<Settings>(cur).map(|s| save(&s)).unwrap_or(false) {
                return 1;
            }
        }
        ["check"] => {
            let (bad, unknown) = Settings::problems(&file_json());
            for k in &bad {
                println!("bad value, default used: {k}");
            }
            for k in &unknown {
                println!("unknown key, ignored: {k}");
            }
            if bad.is_empty() && unknown.is_empty() {
                println!("ok");
            } else {
                return 1;
            }
        }
        _ => {
            println!("{HELP}");
            return 2;
        }
    }
    0
}

