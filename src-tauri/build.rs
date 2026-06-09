fn main() {
    // Bake secrets at compile time for release builds only.
    // Debug reads them at runtime from .env (see lib.rs dotenvy::dotenv() gate).
    // NOTE: this PROFILE gate must stay in sync with the crate's cfg(debug_assertions)
    // gate (see oauth_client_id/secret). They align for the default dev/release profiles;
    // a custom profile that diverges would break the release bake.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        // .env lives at the project root, one level up from this crate.
        // CI sets the vars directly, so a missing file here is benign (env! then fails
        // to *compile* — fail-loud, not a silently broken binary).
        let env_path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("../.env");
        let _ = dotenvy::from_path(&env_path);
        for key in ["ANTIGRAVITY_CLIENT_ID", "ANTIGRAVITY_CLIENT_SECRET"] {
            if let Ok(val) = std::env::var(key) {
                println!("cargo:rustc-env={key}={val}");
            }
            println!("cargo:rerun-if-env-changed={key}");
        }
        println!("cargo:rerun-if-changed={}", env_path.display());
    }
    tauri_build::build()
}
