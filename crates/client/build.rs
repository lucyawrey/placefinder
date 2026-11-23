fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        winresource::WindowsResource::new()
            .set_icon("icons/icon.ico")
            .set("ProductName", "Placefinder")
            .set("FileDescription", "Placefinder")
            .compile()
            .unwrap();
    }
}
