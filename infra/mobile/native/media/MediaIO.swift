import UIKit
import Foundation
import ImageIO

// App-local native callbacks. Rust owns the input/output buffers; no Flutter
// channels, disk staging, photo library writes or key export.
private final class OVGMediaRequest {
    let done = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var ended = false
    private var data: Data?
    func finish(_ bytes: Data?) {
        lock.lock()
        defer { lock.unlock() }
        if ended { return }
        ended = true
        data = bytes
        done.signal()
    }
    func take() -> Data? {
        lock.lock(); defer { lock.unlock() }
        let b = data; data = nil; return b
    }
    var active: Bool { lock.lock(); defer { lock.unlock() }; return !ended }
}
private final class OVGPhotoController: UIViewController {
    var photo: UIImage?
    var closed: (() -> Void)?
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        let image = UIImageView(image: photo)
        image.contentMode = .scaleAspectFit
        image.translatesAutoresizingMaskIntoConstraints = false
        let close = UIButton(type: .system)
        close.setTitle("Close", for: .normal)
        close.translatesAutoresizingMaskIntoConstraints = false
        close.addTarget(self, action: #selector(closeNow), for: .touchUpInside)
        view.addSubview(image); view.addSubview(close)
        NSLayoutConstraint.activate([
            close.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 8),
            close.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
            image.topAnchor.constraint(equalTo: close.bottomAnchor, constant: 8),
            image.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            image.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            image.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor)
        ])
    }
    @objc func closeNow() { dismiss(animated: true) }
    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        photo = nil
        closed?(); closed = nil
    }
}
final class OVGMediaIO: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
    static let shared = OVGMediaIO()
    private let gate = NSLock()
    private var pending: OVGMediaRequest?
    // UIKit objects are accessed on main queue only.
    private var picker: UIImagePickerController?
    private var viewer: OVGPhotoController?
    private var uiRequest: OVGMediaRequest?
    static func install() {
        ovg_ios_media_install(ovgCaptureJPEG, ovgViewJPEG)
        NotificationCenter.default.addObserver(shared, selector: #selector(background), name: UIApplication.didEnterBackgroundNotification, object: nil)
    }
    @objc private func background() {
        uiRequest?.finish(nil)
        picker?.dismiss(animated: false); picker = nil
        viewer?.dismiss(animated: false); viewer = nil
        uiRequest = nil
    }
    private func begin() -> OVGMediaRequest? {
        gate.lock(); defer { gate.unlock() }
        if pending != nil { return nil }
        let r = OVGMediaRequest(); pending = r; return r
    }
    private func end(_ r: OVGMediaRequest) {
        gate.lock(); defer { gate.unlock() }
        if pending === r { pending = nil }
    }
    private func presenter() -> UIViewController? {
        guard UIApplication.shared.applicationState == .active,
              let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first(where: { $0.activationState == .foregroundActive }),
              let root = scene.windows.first(where: { $0.isKeyWindow })?.rootViewController,
              root.presentedViewController == nil else { return nil }
        return root
    }
    func capture() -> Data? {
        guard !Thread.isMainThread, let r = begin() else { return nil }
        defer { end(r) }
        DispatchQueue.main.async {
            guard r.active, let root = self.presenter(), UIImagePickerController.isSourceTypeAvailable(.camera) else { r.finish(nil); return }
            let p = UIImagePickerController()
            p.sourceType = .camera; p.mediaTypes = ["public.image"]; p.allowsEditing = false
            p.delegate = self; p.modalPresentationStyle = .fullScreen
            self.uiRequest = r; self.picker = p
            root.present(p, animated: true)
        }
        if r.done.wait(timeout: .now() + 120) == .timedOut {
            r.finish(nil)
            DispatchQueue.main.async { if self.uiRequest === r { self.background() } }
            return nil
        }
        return r.take()
    }
    func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
        let r = uiRequest; picker.dismiss(animated: true) { r?.finish(nil) }
        self.picker = nil; uiRequest = nil
    }
    func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey : Any]) {
        let r = uiRequest
        var bytes: Data? = nil
        if let image = info[.originalImage] as? UIImage, let cg = image.cgImage,
           cg.width > 0, cg.height > 0, cg.width <= 8192, cg.height <= 8192, cg.width * cg.height <= 32_000_000 {
            if let data = image.jpegData(compressionQuality: 0.92), data.count <= 16 * 1024 * 1024 { bytes = data }
        }
        picker.dismiss(animated: true) { r?.finish(bytes) }
        self.picker = nil; uiRequest = nil
    }
    func view(_ bytes: Data) -> Bool {
        guard !Thread.isMainThread, bytes.count <= 16 * 1024 * 1024,
              let source = CGImageSourceCreateWithData(bytes as CFData, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let w = props[kCGImagePropertyPixelWidth] as? Int, let h = props[kCGImagePropertyPixelHeight] as? Int,
              w > 0, h > 0, w <= 8192, h <= 8192, w*h <= 32_000_000,
              let cg = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways:true,kCGImageSourceThumbnailMaxPixelSize:2048,kCGImageSourceCreateThumbnailWithTransform:true] as CFDictionary),
              let r = begin() else { return false }
        defer { end(r) }
        DispatchQueue.main.async {
            guard r.active, let root = self.presenter() else { r.finish(nil); return }
            let v = OVGPhotoController(); v.photo = UIImage(cgImage: cg)
            v.modalPresentationStyle = .fullScreen
            v.closed = { [weak self] in
                r.finish(Data([1]))
                if self?.uiRequest === r { self?.viewer = nil; self?.uiRequest = nil }
            }
            self.uiRequest = r; self.viewer = v
            root.present(v, animated: true)
        }
        if r.done.wait(timeout: .now() + 120) == .timedOut {
            r.finish(nil)
            DispatchQueue.main.async { if self.uiRequest === r { self.background() } }
            return false
        }
        return r.take() != nil
    }
}
private func ovgCaptureJPEG(_ out: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int32 {
    guard let out = out, capacity > 0, let bytes = OVGMediaIO.shared.capture(),
          bytes.count <= capacity, bytes.count <= Int(Int32.max) else { return -1 }
    bytes.copyBytes(to: out, count: bytes.count)
    return Int32(bytes.count)
}
private func ovgViewJPEG(_ input: UnsafePointer<UInt8>?, _ size: Int) -> Int32 {
    guard let input = input, size > 0, size <= 16 * 1024 * 1024 else { return 0 }
    return OVGMediaIO.shared.view(Data(bytes: input, count: size)) ? 1 : 0
}
