// SPDX-License-Identifier: MIT

import CKaru
import Foundation

/// Policy evaluation result.
public enum Effect: Equatable, CustomStringConvertible {
    case allow
    case deny

    public var description: String {
        switch self {
        case .allow: return "ALLOW"
        case .deny: return "DENY"
        }
    }
}

/// A compiled Karu policy.
///
/// Compile a policy from source, then evaluate it against JSON inputs:
///
///     let policy = try Policy(source: #"allow access if role == "admin";"#)
///     let effect = try policy.evaluate(json: #"{"role": "admin"}"#)
///     assert(effect == .allow)
///
public final class Policy: @unchecked Sendable {
    private var ptr: OpaquePointer?

    /// Compile a Karu policy from source.
    ///
    /// - Parameter source: The Karu policy source code.
    /// - Throws: `KaruError.compileFailed` if the policy cannot be parsed.
    public init(source: String) throws {
        let sourceData = Array(source.utf8)
        let result = sourceData.withUnsafeBufferPointer { buf -> OpaquePointer? in
            guard let base = buf.baseAddress else { return nil }
            return karu_compile(base, UInt(buf.count))
        }
        guard let result else {
            throw KaruError.compileFailed
        }
        self.ptr = result
    }

    deinit {
        if let ptr = ptr {
            karu_policy_free(ptr)
        }
    }

    /// Evaluate this policy against a JSON input string.
    ///
    /// - Parameter json: A valid JSON string representing the authorization request.
    /// - Returns: `.allow` if the policy permits the request, `.deny` otherwise.
    /// - Throws: `KaruError.evaluationFailed` if the input is invalid.
    public func evaluate(json: String) throws -> Effect {
        guard let ptr = ptr else {
            throw KaruError.evaluationFailed
        }
        let jsonData = Array(json.utf8)
        let result = jsonData.withUnsafeBufferPointer { buf -> Int32 in
            guard let base = buf.baseAddress else { return -1 }
            return karu_evaluate(ptr, base, UInt(buf.count))
        }
        switch result {
        case 1: return .allow
        case 0: return .deny
        default: throw KaruError.evaluationFailed
        }
    }
}

/// Compile and evaluate a policy in a single call.
///
/// This is a convenience function for one-shot evaluations.
///
/// - Parameters:
///   - source: The Karu policy source code.
///   - json: A valid JSON string representing the authorization request.
/// - Returns: `.allow` or `.deny`.
/// - Throws: `KaruError.evaluationFailed` on any error.
public func evalOnce(source: String, json: String) throws -> Effect {
    let sourceData = Array(source.utf8)
    let jsonData = Array(json.utf8)

    let result = sourceData.withUnsafeBufferPointer { sBuf -> Int32 in
        jsonData.withUnsafeBufferPointer { jBuf -> Int32 in
            guard let sBase = sBuf.baseAddress, let jBase = jBuf.baseAddress else { return -1 }
            return karu_eval_once(sBase, UInt(sBuf.count), jBase, UInt(jBuf.count))
        }
    }

    switch result {
    case 1: return .allow
    case 0: return .deny
    default: throw KaruError.evaluationFailed
    }
}

/// Errors from Karu operations.
public enum KaruError: Error, CustomStringConvertible {
    case compileFailed
    case evaluationFailed

    public var description: String {
        switch self {
        case .compileFailed: return "Failed to compile Karu policy"
        case .evaluationFailed: return "Policy evaluation failed"
        }
    }
}
