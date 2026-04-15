fn main() {
    let mut res = winresource::WindowsResource::new();
    // 图标还没设计好＞﹏＜
    // res.set_icon("assets/freebird-format-converter.ico");
    // 设置文件属性
    res.set("FileDescription", "A Stardust language interpreter implemented in Rust");
    res.set("ProductName", "Stardust Language Interpreter");
    res.set("LegalCopyright", "copyright © 2026 freebird");
    res.set("FileVersion", "0.1");
    res.set("ProductVersion", "0.1");
    res.compile().unwrap();
}
