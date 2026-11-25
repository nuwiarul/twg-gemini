fn main() {
    println!("cargo:rerun-if-changed=../../packaging/icons/twg-app.ico");

    #[cfg(target_os = "windows")]
    {
        let icon_path = "../../packaging/icons/twg-app.ico";
        if std::path::Path::new(icon_path).exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(icon_path);
            res.set("ProductName", "Tactical Wall Game 3D");
            res.set(
                "FileDescription",
                "Tactical Wall Game 3D — Full Rust Desktop Client",
            );
            res.set("CompanyName", "TWG Team");
            if let Err(err) = res.compile() {
                println!("cargo:warning=Failed to embed Windows icon resource: {err}");
            }
        }
    }
}
