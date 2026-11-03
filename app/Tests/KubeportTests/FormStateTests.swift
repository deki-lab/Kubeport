import XCTest

@testable import Kubeport

final class FormStateTests: XCTestCase {
    private let web = TargetOption(
        kind: .service, name: "web",
        ports: [
            PortOption(port: 8080, name: "http"), PortOption(port: 9090, name: "metrics"),
        ])
    private let db = TargetOption(kind: .service, name: "db", ports: [PortOption(port: 5432, name: nil)])
    private let pod = TargetOption(kind: .pod, name: "db-0", ports: [PortOption(port: 5432, name: nil)])

    private func source(idle: IdleTimeoutView = .inherit) -> ForwardView {
        ForwardView(
            name: "pg", context: "staging", namespace: "data", targetKind: .service,
            targetName: "db", remotePort: 5432, localPort: 15432, active: false, state: nil,
            idleTimeout: idle, idleDeadlineUnix: nil
        )
    }

    func testNewFormPicksTheFirstTargetAndFillsNameAndLocalPort() {
        var form = FormState()
        form.context = "staging"
        form.namespace = "data"
        form.targetsLoaded([web, db, pod])
        form.targetChanged()
        XCTAssertEqual(form.targetName, "web")
        XCTAssertEqual(form.remotePort, "8080")
        XCTAssertEqual(form.name, "web:8080")
        XCTAssertEqual(form.localPort, "8080")
    }

    func testEditedNameAndLocalPortAreKept() {
        var form = FormState()
        form.targetsLoaded([web, db])
        form.targetChanged()
        form.editName("mine")
        form.editLocalPort("18080")
        form.targetName = "db"
        form.targetChanged()
        XCTAssertEqual(form.remotePort, "5432")
        XCTAssertEqual(form.name, "mine")
        XCTAssertEqual(form.localPort, "18080")
    }

    func testSwitchingToPodsPicksAPod() {
        var form = FormState()
        form.targetsLoaded([web, pod])
        form.kind = .pod
        form.kindChanged()
        XCTAssertEqual(form.targetName, "db-0")
        XCTAssertEqual(form.remotePort, "5432")
    }

    func testWithoutAListTheTypedTargetIsKept() {
        var form = FormState()
        form.targetName = "typed"
        form.targetsLoaded([])
        XCTAssertEqual(form.targetName, "typed")
    }

    func testAnEditedForwardKeepsItsValues() {
        var form = FormState(source: source(), name: "pg")
        form.targetsLoaded([web, db])
        XCTAssertEqual(form.targetName, "db")
        XCTAssertEqual(form.remotePort, "5432")
        XCTAssertEqual(form.localPort, "15432")
        XCTAssertEqual(form.name, "pg")
    }

    func testNamespacePrefersTheCurrentOneThenDefault() {
        var form = FormState()
        form.namespace = "data"
        form.namespacesLoaded(["a", "data"])
        XCTAssertEqual(form.namespace, "data")
        form.namespacesLoaded(["a", "default"])
        XCTAssertEqual(form.namespace, "default")
        form.namespacesLoaded(["a"])
        XCTAssertEqual(form.namespace, "a")
    }

    func testInputFollowsTheCoreRules() {
        var form = FormState(source: source(), name: "pg")
        XCTAssertNotNil(form.input)
        form.editName("  ")
        XCTAssertNil(form.input, "blank name")
        form.editName("pg")
        form.editLocalPort("0")
        XCTAssertNil(form.input, "port 0")
        form.editLocalPort("70000")
        XCTAssertNil(form.input, "out of range")
    }

    func testAHandWrittenIdleTimeoutIsOffered() {
        XCTAssertEqual(
            FormState(source: source(idle: .minutes(minutes: 30)), name: "pg").idleChoices, [15, 30, 60, 240])
        XCTAssertEqual(FormState(source: source(idle: .minutes(minutes: 60)), name: "pg").idleChoices, [15, 60, 240])
        XCTAssertEqual(FormState(source: source(idle: .never), name: "pg").idleChoices, [15, 60, 240])
    }
}
