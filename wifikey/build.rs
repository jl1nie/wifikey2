fn main() {
    println!("cargo:rerun-if-changed=cfg.toml");

    // Emit defaults first (overwritten below if cfg.toml is present)
    println!("cargo:rustc-env=CFG_WIFI_SSID=");
    println!("cargo:rustc-env=CFG_WIFI_PASSWORD=");
    println!("cargo:rustc-env=CFG_SERVER_NAME=");
    println!("cargo:rustc-env=CFG_SERVER_PASSWORD=keyer_passwd");
    println!("cargo:rustc-env=CFG_TETHERING=false");

    if let Ok(content) = std::fs::read_to_string("cfg.toml") {
        let mut in_section = false;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[wifikey]" {
                in_section = true;
                continue;
            }
            if line.starts_with('[') {
                in_section = false;
                continue;
            }
            if in_section {
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"');
                    let env_key = match key {
                        "wifi_ssid" => "CFG_WIFI_SSID",
                        "wifi_password" => "CFG_WIFI_PASSWORD",
                        "server_name" => "CFG_SERVER_NAME",
                        "server_password" => "CFG_SERVER_PASSWORD",
                        "tethering" => "CFG_TETHERING",
                        _ => continue,
                    };
                    println!("cargo:rustc-env={env_key}={value}");
                }
            }
        }
    }

    embuild::espidf::sysenv::output();
}
