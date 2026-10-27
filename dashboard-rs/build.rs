//! 构建脚本: macOS 上用系统 swiftc 编译 WKWebView 截图 sidecar(webview_shot),
//! 产物放在主程序同目录(target/<profile>/), 运行时由 current_exe() 同级定位;
//! 打包 .app 时由 make-app.sh 一并拷入 Contents/MacOS/。
//! Windows(含交叉编译)跳过——渲染走 Edge 无头路径, 未来 WebView2 后端在 render.rs 分流。
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=sidecar/webview_shot.swift");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let src = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("sidecar/webview_shot.swift");
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out, 上溯三级即 target/<profile>
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(profile_dir) = out_dir.ancestors().nth(3) else { return };
    let bin = profile_dir.join("webview_shot");
    match Command::new("swiftc").arg("-O").arg(&src).arg("-o").arg(&bin).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => println!(
            "cargo:warning=webview_shot sidecar 编译失败, 渲染将回退 Edge/Chrome: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => println!("cargo:warning=未找到 swiftc, 渲染将回退 Edge/Chrome: {e}"),
    }
}
