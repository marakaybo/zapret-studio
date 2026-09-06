const MANIFEST: &str = include_str!("app.manifest");

fn main() {
    // Две ошибки в манифесте роняют приложение ещё до main(), причём молча и
    // по-разному, поэтому ловим их на сборке, а не на машине пользователя.
    assert!(
        MANIFEST.is_ascii(),
        "app.manifest должен быть только из ASCII: парсер SxS в Windows отвергает \
         кириллицу даже в комментариях, и процесс падает с «Invalid Xml syntax»"
    );
    assert!(
        MANIFEST.contains("Microsoft.Windows.Common-Controls"),
        "app.manifest обязан объявлять зависимость Common-Controls 6, иначе Windows \
         подгрузит старую comctl32 без TaskDialogIndirect и приложение не запустится"
    );

    let windows = tauri_build::WindowsAttributes::new().app_manifest(MANIFEST);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri build script");
}
