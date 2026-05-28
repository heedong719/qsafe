fn main() {
    // ui/ 디렉토리의 모든 파일을 cargo가 추적하도록 명시
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=ui/index.html");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
