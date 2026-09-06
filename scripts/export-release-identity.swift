import Foundation
import Security
import CryptoKit

// Export only the selected signing identity, never the whole login Keychain.
func exportIdentity() throws {
    let env = ProcessInfo.processInfo.environment
    guard let fingerprint = env["JARVIS_EXPORT_IDENTITY"],
          let password = env["JARVIS_EXPORT_PASSWORD"],
          let destination = env["JARVIS_EXPORT_PATH"] else {
        throw NSError(domain: "Missing export configuration", code: 1)
    }
    let query: [String: Any] = [
        kSecClass as String: kSecClassIdentity,
        kSecMatchLimit as String: kSecMatchLimitAll,
        kSecReturnRef as String: true,
    ]
    var result: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &result)
    guard status == errSecSuccess, let identities = result as? [SecIdentity] else {
        throw NSError(domain: "Cannot locate signing identities", code: Int(status))
    }
    for identity in identities {
        var certificate: SecCertificate?
        guard SecIdentityCopyCertificate(identity, &certificate) == errSecSuccess,
              let certificate = certificate else { continue }
        let data = SecCertificateCopyData(certificate) as Data
        let hash = Insecure.SHA1.hash(data: data).map { String(format: "%02X", $0) }.joined()
        guard hash == fingerprint.uppercased() else { continue }
        var parameters = SecItemImportExportKeyParameters()
        parameters.version = UInt32(SEC_KEY_IMPORT_EXPORT_PARAMS_VERSION)
        parameters.passphrase = Unmanaged.passUnretained(password as CFString)
        var exported: CFData?
        let code = SecItemExport(identity, .formatPKCS12, [], &parameters, &exported)
        guard code == errSecSuccess, let exported = exported else {
            throw NSError(domain: "Keychain refused identity export", code: Int(code))
        }
        try (exported as Data).write(to: URL(fileURLWithPath: destination), options: [.atomic])
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: destination)
        return
    }
    throw NSError(domain: "Selected identity was not found", code: 1)
}
do { try exportIdentity() }
catch { fputs("Não foi possível exportar a identidade selecionada: \(error.localizedDescription)\n", stderr); exit(1) }
