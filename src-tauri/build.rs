fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile == "release" && !std::path::Path::new("../dist/index.html").is_file() {
        panic!(
            "Release build requires ../dist/index.html. Use `pnpm tauri build` (or `npm run tauri build`) instead of bare `cargo build` so frontend assets are bundled instead of loading http://localhost:1420."
        );
    }
    tauri_build::build()
}
