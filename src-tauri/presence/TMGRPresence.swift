import AppKit
import CryptoKit
import Foundation
import LocalAuthentication

private let lock = NSLock()
private var epoch: UInt64 = 0
private var observers: [NSObjectProtocol] = []

private func bump(_: Notification) {
  lock.lock()
  epoch &+= 1
  lock.unlock()
}

private func copy(_ data: Data, _ buf: UnsafeMutablePointer<UInt8>, _ cap: Int, _ len: UnsafeMutablePointer<Int>) -> Int32 {
  guard data.count <= cap else { return -2 }
  data.copyBytes(to: buf, count: data.count)
  len.pointee = data.count
  return 0
}

@_cdecl("tmgr_presence_available")
public func tmgr_presence_available() -> Int32 {
  SecureEnclave.isAvailable ? 1 : 0
}

@_cdecl("tmgr_presence_watch")
public func tmgr_presence_watch() {
  guard observers.isEmpty else { return }
  let ws = NSWorkspace.shared.notificationCenter
  for name in [NSWorkspace.willSleepNotification, NSWorkspace.screensDidSleepNotification, NSWorkspace.sessionDidResignActiveNotification] {
    observers.append(ws.addObserver(forName: name, object: nil, queue: .main, using: bump))
  }
  observers.append(DistributedNotificationCenter.default().addObserver(forName: Notification.Name("com.apple.screenIsLocked"), object: nil, queue: .main, using: bump))
}

@_cdecl("tmgr_presence_epoch")
public func tmgr_presence_epoch() -> UInt64 {
  lock.lock()
  defer { lock.unlock() }
  return epoch
}

@_cdecl("tmgr_presence_create")
public func tmgr_presence_create(_ blob: UnsafeMutablePointer<UInt8>, _ cap: Int, _ blobLen: UnsafeMutablePointer<Int>, _ pub: UnsafeMutablePointer<UInt8>) -> Int32 {
  guard let access = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, [.privateKeyUsage, .userPresence], nil),
        let key = try? SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
  else { return -1 }
  let raw = key.publicKey.x963Representation
  guard raw.count == 65 else { return -1 }
  raw.copyBytes(to: pub, count: 65)
  return copy(key.dataRepresentation, blob, cap, blobLen)
}

@_cdecl("tmgr_presence_public")
public func tmgr_presence_public(_ blob: UnsafePointer<UInt8>, _ len: Int, _ pub: UnsafeMutablePointer<UInt8>) -> Int32 {
  guard let key = try? SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: Data(bytes: blob, count: len)) else { return -3 }
  let raw = key.publicKey.x963Representation
  guard raw.count == 65 else { return -3 }
  raw.copyBytes(to: pub, count: 65)
  return 0
}

@_cdecl("tmgr_presence_evaluate")
public func tmgr_presence_evaluate(_ reason: UnsafePointer<CChar>, _ out: UnsafeMutablePointer<UnsafeMutableRawPointer?>) -> Int32 {
  let ctx = LAContext()
  let done = DispatchSemaphore(value: 0)
  var ok = false
  var code = 0
  ctx.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: String(cString: reason)) { success, error in
    ok = success
    code = (error as? LAError)?.code.rawValue ?? 0
    done.signal()
  }
  done.wait()
  guard ok else {
    ctx.invalidate()
    let cancelled = [LAError.userCancel.rawValue, LAError.systemCancel.rawValue, LAError.appCancel.rawValue].contains(code)
    return cancelled ? -4 : -5
  }
  ctx.interactionNotAllowed = true
  out.pointee = Unmanaged.passRetained(ctx).toOpaque()
  return 0
}

@_cdecl("tmgr_presence_sign")
public func tmgr_presence_sign(_ ctx: UnsafeMutableRawPointer, _ blob: UnsafePointer<UInt8>, _ blobLen: Int, _ msg: UnsafePointer<UInt8>, _ msgLen: Int, _ der: UnsafeMutablePointer<UInt8>, _ cap: Int, _ derLen: UnsafeMutablePointer<Int>) -> Int32 {
  let context = Unmanaged<LAContext>.fromOpaque(ctx).takeUnretainedValue()
  guard let key = try? SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: Data(bytes: blob, count: blobLen), authenticationContext: context) else { return -3 }
  guard let sig = try? key.signature(for: Data(bytes: msg, count: msgLen)) else { return -6 }
  return copy(sig.derRepresentation, der, cap, derLen)
}

@_cdecl("tmgr_presence_release")
public func tmgr_presence_release(_ ctx: UnsafeMutableRawPointer) {
  Unmanaged<LAContext>.fromOpaque(ctx).takeRetainedValue().invalidate()
}
