fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("nant.png");
        res.set("FileDescription", "Stardust Language Interpreter");
        res.set("ProductName", "Stardust Language Interpreter");
        res.set("LegalCopyright", "Copyright © 2026 freebird");
        res.set("FileVersion", "0.1.6");
        res.set("ProductVersion", "0.1.6");
        res.compile().unwrap();
    }
}
