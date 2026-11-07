fn main() {
    let out = std::env::var("OUT_DIR").unwrap_or_default();
    for name in ["en", "fa"] {
        let src = std::path::Path::new("lang").join(format!("{}.ttf", name));
        let dst = std::path::Path::new(&out).join(format!("{}.ttf", name));
        if src.exists() {
            let _ = std::fs::copy(&src, &dst);
        } else {
            let _ = std::fs::write(&dst, b"");
        }
        println!("cargo:rerun-if-changed=lang/{}.ttf", name);
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("allchats.ico");
        let _ = res.compile();
    }
}
