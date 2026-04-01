// SPDX-License-Identifier: MIT

import XCTest
@testable import Karu

final class KaruTests: XCTestCase {
    func testCompileAndEvaluate() throws {
        let policy = try Policy(source: #"allow access if role == "admin";"#)
        let allow = try policy.evaluate(json: #"{"role": "admin"}"#)
        XCTAssertEqual(allow, .allow)

        let deny = try policy.evaluate(json: #"{"role": "user"}"#)
        XCTAssertEqual(deny, .deny)
    }

    func testEvalOnce() throws {
        let effect = try evalOnce(
            source: #"allow access if value > 10;"#,
            json: #"{"value": 15}"#
        )
        XCTAssertEqual(effect, .allow)
    }

    func testCompileError() {
        XCTAssertThrowsError(try Policy(source: "allow if if if {{{"))
    }

    func testEffectDescription() {
        XCTAssertEqual(Effect.allow.description, "ALLOW")
        XCTAssertEqual(Effect.deny.description, "DENY")
    }
}
