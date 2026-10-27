// webview_shot: 用系统自带 WKWebView 离屏渲染 HTML 并截图(零安装, 无需 Edge/Chrome)。
// 由 build.rs 在 macOS 上用 swiftc 编译, 与主程序同目录分发; Windows 构建不包含本工具。
// 用法: webview_shot <in.html> <out.png> <宽> <高> [超时秒=20]
// 流程: loadFileURL -> didFinish -> 等 300ms 让内联脚本/字体稳定 -> takeSnapshot
//       -> 重采样到 1x 目标尺寸(离屏视图默认跟随主屏 Retina) -> 写 PNG -> 进程退出。
// 渲染失败(超时/导航失败)以非零码退出, 由主程序回退到浏览器截图路径。
import WebKit
import AppKit

guard CommandLine.arguments.count >= 5,
      let w = Int(CommandLine.arguments[3]),
      let h = Int(CommandLine.arguments[4]) else {
    print("usage: webview_shot <in.html> <out.png> <w> <h> [timeout_s]")
    exit(2)
}
let inURL  = URL(fileURLWithPath: CommandLine.arguments[1])
let outURL = URL(fileURLWithPath: CommandLine.arguments[2])
let width  = CGFloat(w), height = CGFloat(h)
let timeout: Double = CommandLine.arguments.count > 5 ? Double(CommandLine.arguments[5]) ?? 20 : 20

final class Delegate: NSObject, WKNavigationDelegate {
    weak var wv: WKWebView?
    var done = false

    func webView(_ wv: WKWebView, didFinish nav: WKNavigation!) {
        guard !done else { return }
        done = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { self.snap() }
    }

    func webView(_ wv: WKWebView, didFail nav: WKNavigation!, withError error: Error) {
        print("navigation failed: \(error.localizedDescription)")
        exit(1)
    }

    private func snap() {
        guard let wv = self.wv else { exit(1) }
        let cfg = WKSnapshotConfiguration()
        cfg.rect = CGRect(x: 0, y: 0, width: wv.frame.width, height: wv.frame.height)
        wv.takeSnapshot(with: cfg) { img, err in
            guard let nsimg = img else {
                print("snapshot error: \(err?.localizedDescription ?? "?")")
                exit(1)
            }
            let cg = nsimg.cgImage(forProposedRect: nil, context: nil, hints: nil)!
            let ctx = CGContext(data: nil, width: Int(width), height: Int(height),
                                bitsPerComponent: 8, bytesPerRow: 0,
                                space: CGColorSpaceCreateDeviceRGB(),
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            ctx.interpolationQuality = .high
            ctx.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
            let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
            guard let png = rep.representation(using: .png, properties: [:]) else {
                print("png encode error"); exit(1)
            }
            do { try png.write(to: outURL) } catch {
                print("write error: \(error)"); exit(1)
            }
            exit(0)
        }
    }
}

let app = NSApplication.shared
let wv = WKWebView(frame: NSRect(x: 0, y: 0, width: width, height: height))
let del = Delegate(); del.wv = wv
wv.navigationDelegate = del
wv.loadFileURL(inURL, allowingReadAccessTo: inURL.deletingLastPathComponent())

// 超时守卫: 没等到 didFinish 就以非零码退出, 主程序据此回退浏览器路径
DispatchQueue.main.asyncAfter(deadline: .now() + timeout) {
    if !del.done { print("timeout after \(timeout)s"); exit(3) }
}
app.run()
