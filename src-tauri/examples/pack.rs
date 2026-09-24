fn main() {
    let url = std::env::args().nth(1).expect("usage: pack <mrpack-url> <filename> <name>");
    let filename = std::env::args().nth(2).unwrap_or_else(|| "pack.mrpack".into());
    let name = std::env::args().nth(3).unwrap_or_else(|| "Pack Test".into());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let id = rt
        .block_on(debang_launcher_lib::modpack::install_pack(None, url, filename, name, None))
        .unwrap();
    println!("installed instance id: {}", id);
}
