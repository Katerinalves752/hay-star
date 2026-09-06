// ============================================================================
//  HAY STAR - Configuration constants
// ============================================================================

pub const PACKAGE_NAME: &str = "com.supercell.hayday";

// --- Remote device paths ---
pub const ASSET_VAULT_PRIMARY:  &str = "/data/adb/mstar-assets";
pub const ASSET_VAULT_FALLBACK: &str = "/data/adb/nxrth-assets";
pub const ASSET_VAULT:          &str = "/data/adb/mstar-assets";
pub const FRIDA_BIN:   &str = "/data/adb/mstar-assets/.service";
pub const GADGET_VAULT: &str = "/data/adb/mstar-assets/libmetrics.so";

pub fn app_data_dir() -> String {
    format!("/data/user/0/{PACKAGE_NAME}")
}
pub fn runtime_gadget_dir() -> String {
    format!("{}/files/metrics", app_data_dir())
}
pub fn gadget_bin() -> String {
    format!("{}/libmetrics.so", runtime_gadget_dir())
}
pub fn gadget_config_remote() -> String {
    let gb = gadget_bin();
    if gb.ends_with(".so") {
        format!("{}.config.so", &gb[..gb.len()-3])
    } else {
        format!("{}.config", gb)
    }
}
pub const CONFIG_STAGE_PATH: &str = "/data/local/tmp/.mstar-libmetrics.config.so";

pub fn module_remote() -> String {
    format!("{}/files/libmstar.so", app_data_dir())
}
pub const MODULE_STAGE: &str = "/data/local/tmp/.mstar-libmstar.so";
pub const INJECTOR_OAT_DIR: &str = "/data/local/tmp/oat";
pub const SERVER_LOG: &str = "/data/local/tmp/.mstar-server.log";

// --- SHA-256 checksums of the Frida binaries in the asset vault ---
pub const FRIDA_SHA256:  &str = "83f200183bcee2a73626474b5bb488631af74b2a582f1906474cf78fc7f2c61c";
pub const GADGET_SHA256: &str = "cfd21e76394bcf86481707754720c3d279016066e71aadeeef26d6ecdff4f981";

pub const FRIDA_SHA256_LIST: &[&str] = &[
    "83f200183bcee2a73626474b5bb488631af74b2a582f1906474cf78fc7f2c61c",
    "b13013c5fb19b01dc81fed1cd9b517b10681240c63d8111167e9df50fb0a0d18",
];

pub const GADGET_SHA256_LIST: &[&str] = &[
    "cfd21e76394bcf86481707754720c3d279016066e71aadeeef26d6ecdff4f981",
];

// --- Ports ---
pub const FRIDA_PORT:   u16 = 31337;
pub const GADGET_PORT:  u16 = 31338;
pub const CONTROL_PORT: u16 = 31350;

// --- Timeouts (seconds) ---
pub const CONNECT_TIMEOUT:   f64 = 12.0;
pub const RESUME_TIMEOUT:    f64 =  5.0;
pub const ENGINE_TIMEOUT:    f64 = 90.0;
pub const RPC_TIMEOUT:       f64 =  5.0;
pub const STABILITY_WINDOW:  f64 =  5.0;

// --- ADB search paths ---
pub const ADB_SEARCH_PATHS: &[&str] = &[
    r"C:\LDPlayer\LDPlayer9\adb.exe",
    r"C:\LDPlayer\LDPlayer14\adb.exe",
    r"C:\Program Files\Genymobile\Genymotion\tools\adb.exe",
    "adb",
];

// --- Game offsets (Hay Day 1.72.2 on LDPlayer 9, ARM64 libg.so) ---
pub const FIELD_VTABLE_OFF:       u32 = 0x14b8770;
pub const FIELD_STRIDE:           u64 = 0x200;
pub const TICK_FUNC_OFF:          u32 = 0x00ae2430;
pub const TICK_GOT_OFF:           u32 = 0x015057f0;
pub const TRY_EXEC_CMD_OFF:       u32 = 0x00ae3bc4;
pub const NEW_OFF:                u32 = 0x141c480;
pub const WHEAT_ITEM:             u32 = 400001;

// --- Frida target library ---
pub const TARGET_LIB: &str = "libg.so";
pub const TARGET_FILE_SIZE: u64 = 23415808;