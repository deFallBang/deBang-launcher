use std::path::PathBuf;

fn main() {
    let mc = std::env::args().nth(1).expect("usage: prep <version> [loader]");
    let loader = std::env::args().nth(2).unwrap_or_else(|| "Vanilla".into());
    let inst = PathBuf::from(std::env::var("HOME").unwrap())
        .join(".local/share/debang-launcher/instances/_prep_test");
    std::fs::create_dir_all(inst.join("mods")).ok();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let p = rt
        .block_on(debang_launcher_lib::versions::prepare(
            |ph, d, t| {
                if d == t || d % 2500 == 0 {
                    println!("[{:>14}] {}/{}", ph, d, t);
                }
            },
            |line| println!("{}", line),
            &mc,
            &loader,
            &inst,
            "8f14e45f-ceea-467a-a1b9-3c8f0d0e5f11",
            "deBangPlayer",
            None,
        ))
        .unwrap();
    println!("── java major : {}", p.java_major);
    println!("── mainClass  : {}", p.main_class);
    println!("── jvm args   : {}", p.jvm.join(" "));
    println!("── game args  : {}", p.game.join(" "));
    println!("── classpath   {} entries", p.classpath.as_ref().map(|c| c.split(':').count()).unwrap_or(0));
    if std::env::args().nth(3).as_deref() == Some("--exec") {
        let java = std::env::args().nth(4).unwrap_or_else(|| "/usr/lib/jvm/java-21-openjdk/bin/java".into());
        let mem = std::env::args().nth(5).unwrap_or_else(|| "2048".into());
        let mut a: Vec<String> = vec![format!("-Xms512M"), format!("-Xmx{}M", mem)];
        a.extend(p.jvm.iter().cloned());
        if let Some(cp) = &p.classpath {
            a.push("-cp".into());
            a.push(cp.clone());
        }
        a.push(p.main_class.clone());
        a.extend(p.game.iter().cloned());
        println!("EXEC {} ...", java);
        let mut child = std::process::Command::new(&java)
            .args(&a)
            .current_dir(&inst)
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(22));
        let _ = child.kill();
        println!("EXEC DONE (game process was alive for 22s)");
        return;
    }
    println!("OK");
}
