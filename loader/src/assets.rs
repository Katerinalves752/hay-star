pub fn resolve_asset_vault(adb: &str, device_id: &str) -> String {
    for vault in &[ASSET_VAULT_PRIMARY, ASSET_VAULT_FALLBACK] {
        let bin = format!("{vault}/.service");
        if let Ok(c) = su_command(adb, device_id, &format!("test -x {}", shell_quote(&bin)), false) {
            if c.status.code() == Some(0) {
                return (*vault).to_string();
            }
        }
    }
    ASSET_VAULT_PRIMARY.to_string()
}
// ============================================================================
//  HAY STAR - Asset verification and staging
//  Mirrors MStarConsole.prepare_assets(), _stage_runtime_gadget(),
//  _stage_native_module() in loader.py
// ============================================================================

use std::path::Path;
use sha2::{Sha256, Digest};
use crate::adb::*;
use crate::config::*;
use crate::error::{LoaderError, Result};

/// Verify a local file's SHA-256 matches expected.
pub fn verify_local_sha256(path: &Path, expected: &str) -> Result<()> {
    let data = std::fs::read(path)
        .map_err(|e| LoaderError::Asset(format!("cannot read {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let digest = hex::encode(hasher.finalize());
    if digest != expected {
        return Err(LoaderError::Asset(format!(
            "local file {} SHA-256 mismatch:\n  expected {expected}\n  got      {digest}",
            path.display()
        )));
    }
    Ok(())
}

/// Prepare and verify all assets before injection.
/// Mirrors MStarConsole.prepare_assets().
pub fn prepare_assets(
    adb: &str,
    device_id: &str,
    project_dir: &Path,
    gadget_config_path: &Path,
    server_exe: &mut String,
) -> Result<()> {
    let hook_path = project_dir.join("hook.js");
    if !hook_path.is_file() {
        return Err(LoaderError::Asset(format!("hook script not found: {}", hook_path.display())));
    }
    if !gadget_config_path.is_file() {
        return Err(LoaderError::Asset(format!("gadget config not found: {}", gadget_config_path.display())));
    }

    // Validate gadget config JSON
    let config_text = std::fs::read_to_string(gadget_config_path)
        .map_err(|e| LoaderError::Asset(format!("cannot read gadget config: {e}")))?;
    let config: serde_json::Value = serde_json::from_str(&config_text)
        .map_err(|e| LoaderError::Asset(format!("invalid gadget config JSON: {e}")))?;
    let interaction = config.get("interaction")
        .ok_or_else(|| LoaderError::Asset("gadget config missing 'interaction'".into()))?;
    if interaction.get("type").and_then(|v| v.as_str()) != Some("listen") {
        return Err(LoaderError::Asset("gadget config must use listen interaction".into()));
    }
    if interaction.get("port").and_then(|v| v.as_u64()) != Some(GADGET_PORT as u64) {
        return Err(LoaderError::Asset(format!("gadget config must listen on port {GADGET_PORT}")));
    }
    if interaction.get("on_load").and_then(|v| v.as_str()) != Some("resume") {
        return Err(LoaderError::Asset("gadget config on_load must be 'resume'".into()));
    }

    let vault = resolve_asset_vault(adb, device_id);
    let _ = su_command(adb, device_id, "if [ ! -e /data/adb/mstar-assets ] && [ -d /data/adb/nxrth-assets ]; then ln -s /data/adb/nxrth-assets /data/adb/mstar-assets; fi", false);
    let frida_bin = format!("{vault}/.service");
    let gadget_vault = format!("{vault}/libmetrics.so");

    // Check Frida server exists and is executable on device
    let check = su_command(adb, device_id, &format!("test -x {}", shell_quote(&frida_bin)), false)?;
    if check.status.code() != Some(0) {
        return Err(LoaderError::Asset(format!(
            "Frida server missing or not executable at {frida_bin}\n  (also checked {ASSET_VAULT_FALLBACK}/.service).\n  Please place .service and libmetrics.so into {vault}/ as described in README."
        )));
    }

    // Check gadget vault
    let check = su_command(adb, device_id, &format!("test -r {}", shell_quote(&gadget_vault)), false)?;
    if check.status.code() != Some(0) {
        return Err(LoaderError::Asset(format!("Frida gadget missing: {gadget_vault}")));
    }

    // Verify SHA-256 of remote binaries
    println!("[*] Verifying remote asset checksums ({vault})...");
    let server_digest = remote_sha256(adb, device_id, &frida_bin)?;
    if !FRIDA_SHA256_LIST.contains(&server_digest.as_str()) {
        return Err(LoaderError::Asset(format!(
            "Frida server SHA-256 mismatch:\n  expected one of {:?}\n  got      {server_digest}",
            FRIDA_SHA256_LIST
        )));
    }
    let gadget_digest = remote_sha256(adb, device_id, &gadget_vault)?;
    if !GADGET_SHA256_LIST.contains(&gadget_digest.as_str()) {
        return Err(LoaderError::Asset(format!(
            "Frida gadget SHA-256 mismatch:\n  expected one of {:?}\n  got      {gadget_digest}",
            GADGET_SHA256_LIST
        )));
    }

    *server_exe = remote_realpath(adb, device_id, &frida_bin);
    Ok(())
}

/// Stage the Frida gadget (libmetrics.so) into the app's files directory
/// with correct uid/chmod/chcon so the app can load it.
pub fn stage_runtime_gadget(
    adb: &str,
    device_id: &str,
    gadget_config_path: &Path,
    app_uid: u32,
    app_context: &str,
) -> Result<()> {
    let gb = gadget_bin();
    let gc = gadget_config_remote();
    let gadget_tmp = format!("{gb}.mstar-tmp");
    let config_tmp = format!("{gc}.mstar-tmp");
    let rgd = runtime_gadget_dir();

    // Push gadget config to staging area
    crate::adb::adb_checked(adb, device_id, &[
        "push",
        gadget_config_path.to_str().unwrap_or(""),
        CONFIG_STAGE_PATH,
    ])?;

    let ctx = shell_quote(app_context);
    let uid = app_uid; let vault = resolve_asset_vault(adb, device_id); let gadget_src = format!("{vault}/libmetrics.so");
    let cmd = [
        format!("rm -f {} {}", shell_quote(&gadget_tmp), shell_quote(&config_tmp)),
        format!("mkdir -p {}", shell_quote(&rgd)),
        format!("cp -f {} {}", shell_quote(&gadget_src), shell_quote(&gadget_tmp)),
        format!("cp -f {} {}", shell_quote(CONFIG_STAGE_PATH), shell_quote(&config_tmp)),
        format!("chown {uid}:{uid} {} {} {}", shell_quote(&rgd), shell_quote(&gadget_tmp), shell_quote(&config_tmp)),
        format!("chmod 0700 {}", shell_quote(&rgd)),
        format!("chmod 0500 {}", shell_quote(&gadget_tmp)),
        format!("chmod 0400 {}", shell_quote(&config_tmp)),
        format!("chcon {ctx} {} {} {}", shell_quote(&rgd), shell_quote(&gadget_tmp), shell_quote(&config_tmp)),
        format!("mv -f {} {}", shell_quote(&gadget_tmp), shell_quote(&gb)),
        format!("mv -f {} {}", shell_quote(&config_tmp), shell_quote(&gc)),
        format!("chown {uid}:{uid} {} {}", shell_quote(&gb), shell_quote(&gc)),
        format!("chmod 0500 {}", shell_quote(&gb)),
        format!("chmod 0400 {}", shell_quote(&gc)),
        format!("chcon {ctx} {} {}", shell_quote(&gb), shell_quote(&gc)),
    ].join(" && ");

    let _ = su_command(adb, device_id, &cmd, false);
    // Cleanup staging files
    let _ = su_command(adb, device_id, &format!(
        "rm -f {} {} {}",
        shell_quote(CONFIG_STAGE_PATH),
        shell_quote(&gadget_tmp),
        shell_quote(&config_tmp),
    ), false);

    // Verify staged files
    verify_runtime_path(adb, device_id, &rgd, "700", true, app_uid, app_context)?;
    verify_runtime_path(adb, device_id, &gb, "500", false, app_uid, app_context)?;
    verify_runtime_path(adb, device_id, &gc, "400", false, app_uid, app_context)?;

    // Verify SHA-256 of staged gadget
    let staged_digest = remote_sha256(adb, device_id, &gb)?;
    if !GADGET_SHA256_LIST.contains(&staged_digest.as_str()) {
        return Err(LoaderError::Asset(format!(
            "staged gadget SHA-256 mismatch:\n  expected one of {:?}\n  got      {staged_digest}",
            GADGET_SHA256_LIST
        )));
    }
    println!("[+] Runtime gadget staged for UID {app_uid}: {gb}");
    Ok(())
}

fn verify_runtime_path(
    adb: &str,
    device_id: &str,
    path: &str,
    expected_mode: &str,
    is_dir: bool,
    app_uid: u32,
    app_context: &str,
) -> Result<()> {
    let kind = if is_dir { "-d" } else { "-s" };
    let exists = su_command(adb, device_id, &format!("test {kind} {}", shell_quote(path)), false)?;
    if exists.status.code() != Some(0) {
        return Err(LoaderError::Asset(format!("staged runtime asset missing: {path}")));
    }
    let out = su_command(adb, device_id, &format!("stat -c '%u %a' {}", shell_quote(path)), false)?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(LoaderError::Asset(format!("unexpected stat output for: {path}")));
    }
    let (uid, mode) = (parts[0], parts[1]);
    let context = remote_context(adb, device_id, path);
    if uid != app_uid.to_string() || mode != expected_mode || context.as_deref() != Some(app_context) {
        return Err(LoaderError::Asset(format!(
            "invalid owner/mode/context for {path}: uid={uid}, mode={mode}, context={:?}",
            context
        )));
    }
    Ok(())
}

/// Stage the native engine module (libmstar.so) into the app's files directory.
pub fn stage_native_module(
    adb: &str,
    device_id: &str,
    module_local: &Path,
    app_uid: u32,
    app_context: &str,
) -> Result<()> {
    if !module_local.is_file() {
        return Err(LoaderError::Asset(format!(
            "native module not built: {} (run native/build.ps1)",
            module_local.display()
        )));
    }
    let mr = module_remote();
    let tmp = format!("{mr}.mstar-tmp");
    let files_dir = format!("{}/files", app_data_dir());
    let uid = app_uid; let vault = resolve_asset_vault(adb, device_id); let gadget_src = format!("{vault}/libmetrics.so");
    let ctx = shell_quote(app_context);

    crate::adb::adb_checked(adb, device_id, &[
        "push",
        module_local.to_str().unwrap_or(""),
        MODULE_STAGE,
    ])?;

    let cmd = [
        format!("mkdir -p {}", shell_quote(&files_dir)),
        format!("rm -f {}", shell_quote(&tmp)),
        format!("cp -f {} {}", shell_quote(MODULE_STAGE), shell_quote(&tmp)),
        format!("chown {uid}:{uid} {}", shell_quote(&tmp)),
        format!("chmod 0500 {}", shell_quote(&tmp)),
        format!("chcon {ctx} {}", shell_quote(&tmp)),
        format!("mv -f {} {}", shell_quote(&tmp), shell_quote(&mr)),
    ].join(" && ");

    su_command(adb, device_id, &cmd, false)?;
    let _ = su_command(adb, device_id, &format!("rm -f {}", shell_quote(MODULE_STAGE)), false);
    verify_runtime_path(adb, device_id, &mr, "500", false, app_uid, app_context)?;
    println!("[+] Native module staged: {mr}");
    Ok(())
}
