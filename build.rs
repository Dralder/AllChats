fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("allchats.ico");
        let _ = res.compile();
    }
}
