// Platform service file generation for `opake daemon install/uninstall`.
//
// macOS: launchd plist in ~/Library/LaunchAgents/
// Linux: systemd user unit in ~/.config/systemd/user/

use std::path::PathBuf;

use anyhow::Result;

use crate::config::FileStorage;

#[cfg(target_os = "macos")]
pub fn install(storage: &FileStorage) -> Result<()> {
    let binary = std::env::current_exe()?;
    install_launchd(&binary.display().to_string(), storage)
}

#[cfg(target_os = "linux")]
pub fn install(_storage: &FileStorage) -> Result<()> {
    let binary = std::env::current_exe()?;
    install_systemd(&binary.display().to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn install(_storage: &FileStorage) -> Result<()> {
    anyhow::bail!(
        "automatic service installation is not supported on this platform.\n\
         Run `opake daemon run` manually or via your OS task scheduler."
    );
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<()> {
    uninstall_launchd()
}

#[cfg(target_os = "linux")]
pub fn uninstall() -> Result<()> {
    uninstall_systemd()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn uninstall() -> Result<()> {
    anyhow::bail!("automatic service uninstallation is not supported on this platform.");
}

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn home_dir() -> Result<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| anyhow::anyhow!("cannot determine home directory ($HOME not set)"))
}

// ---------------------------------------------------------------------------
// macOS — launchd
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
const LAUNCHD_LABEL: &str = "app.opake.daemon";

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(target_os = "macos")]
fn launchd_plist_path() -> Result<PathBuf> {
    Ok(home_dir()?
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn install_launchd(binary_path: &str, storage: &FileStorage) -> Result<()> {
    let plist_path = launchd_plist_path()?;
    let log_dir = storage.base_dir().join("logs");

    std::fs::create_dir_all(&log_dir)?;
    std::fs::create_dir_all(plist_path.parent().unwrap())?;

    let bin = xml_escape(binary_path);
    let stdout = xml_escape(&log_dir.join("daemon.log").display().to_string());
    let stderr = xml_escape(&log_dir.join("daemon.err").display().to_string());

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>daemon</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
</dict>
</plist>"#,
    );

    std::fs::write(&plist_path, plist)?;
    println!("Service file written to {}", plist_path.display());
    println!();
    println!("  launchctl load {}", plist_path.display());
    println!();
    println!("To check status:");
    println!("  launchctl list | grep opake");

    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd() -> Result<()> {
    let plist_path = launchd_plist_path()?;
    if plist_path.exists() {
        println!("Unload the service first if it's running:");
        println!();
        println!("  launchctl unload {}", plist_path.display());
        println!();
        std::fs::remove_file(&plist_path)?;
        println!("Removed {}", plist_path.display());
    } else {
        println!("No service file found at {}", plist_path.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Linux — systemd
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
const SYSTEMD_SERVICE_NAME: &str = "opake-daemon.service";

#[cfg(target_os = "linux")]
fn systemd_unit_path() -> Result<PathBuf> {
    Ok(home_dir()?
        .join(".config")
        .join("systemd")
        .join("user")
        .join(SYSTEMD_SERVICE_NAME))
}

#[cfg(target_os = "linux")]
fn install_systemd(binary_path: &str) -> Result<()> {
    let unit_path = systemd_unit_path()?;
    std::fs::create_dir_all(unit_path.parent().unwrap())?;

    let unit = format!(
        r#"[Unit]
Description=Opake session daemon
After=network-online.target
Wants=network-online.target

[Service]
ExecStart={binary_path} daemon run
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
"#
    );

    std::fs::write(&unit_path, unit)?;
    println!("Service file written to {}", unit_path.display());
    println!();
    println!("  systemctl --user enable --now opake-daemon");
    println!();
    println!("To check status:");
    println!("  systemctl --user status opake-daemon");

    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_systemd() -> Result<()> {
    let unit_path = systemd_unit_path()?;
    if unit_path.exists() {
        std::fs::remove_file(&unit_path)?;
        println!("Removed {}", unit_path.display());
        println!();
        println!("  systemctl --user disable --now opake-daemon");
        println!("  systemctl --user daemon-reload");
    } else {
        println!("No service file found at {}", unit_path.display());
    }
    Ok(())
}
