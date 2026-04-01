// SPDX-License-Identifier: MIT

const vscode = require('vscode');
const path = require('path');
const fs = require('fs');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;
let passDecorationType;
let failDecorationType;
let coverageFullDecorationType;
let coveragePartialDecorationType;
let coverageNoneDecorationType;

const BUNDLED_SERVER_PATHS = {
    'linux-x64': { bundleDir: 'linux-x64', binaryName: 'karu-lsp' },
    'linux-arm64': { bundleDir: 'linux-arm64', binaryName: 'karu-lsp' },
    'darwin-x64': { bundleDir: 'darwin-x64', binaryName: 'karu-lsp' },
    'darwin-arm64': { bundleDir: 'darwin-arm64', binaryName: 'karu-lsp' },
    'win32-x64': { bundleDir: 'win32-x64', binaryName: 'karu-lsp.exe' },
    'win32-arm64': { bundleDir: 'win32-arm64', binaryName: 'karu-lsp.exe' },
};

function hostBinaryName() {
    return process.platform === 'win32' ? 'karu-lsp.exe' : 'karu-lsp';
}

function ensureExecutable(serverPath) {
    if (process.platform === 'win32' || !fs.existsSync(serverPath)) {
        return;
    }

    try {
        fs.chmodSync(serverPath, 0o755);
    } catch (error) {
        console.warn(`Karu LSP: failed to chmod ${serverPath}: ${error.message}`);
    }
}

function findBundledServerPath(extensionPath) {
    const bundle = BUNDLED_SERVER_PATHS[`${process.platform}-${process.arch}`];
    if (!bundle) {
        return null;
    }

    const bundledPath = path.join(extensionPath, 'bin', bundle.bundleDir, bundle.binaryName);
    if (!fs.existsSync(bundledPath)) {
        return null;
    }

    ensureExecutable(bundledPath);
    console.log('Karu LSP: using bundled server');
    return bundledPath;
}

function findServerPath(extensionPath) {
    const config = vscode.workspace.getConfiguration('karu');
    const configPath = config.get('serverPath');
    const binaryName = hostBinaryName();

    // 1. Use configured path if set
    if (configPath && configPath.length > 0) {
        console.log('Karu LSP: using configured path');
        return configPath;
    }

    // 2. Use the bundled binary shipped inside the VSIX when available
    const bundledPath = findBundledServerPath(extensionPath);
    if (bundledPath) {
        return bundledPath;
    }

    // 3. Try relative to the crate (for ad-hoc local builds)
    const crateRoot = path.resolve(extensionPath, '..', '..');
    const debugFromCrate = path.join(crateRoot, 'target', 'debug', binaryName);
    const releaseFromCrate = path.join(crateRoot, 'target', 'release', binaryName);

    // Prefer debug for development (faster builds, debug symbols)
    if (fs.existsSync(debugFromCrate)) {
        console.log('Karu LSP: found debug binary relative to crate');
        return debugFromCrate;
    }
    if (fs.existsSync(releaseFromCrate)) {
        console.log('Karu LSP: found release binary relative to crate');
        return releaseFromCrate;
    }

    // 3b. Try the Cargo workspace root (works for both the standalone Karu repo
    // and the parent Kodus repo with Karu vendored as a submodule).
    const workspaceRoot = path.resolve(crateRoot, '..', '..');
    const debugFromRoot = path.join(workspaceRoot, 'target', 'debug', binaryName);
    const releaseFromRoot = path.join(workspaceRoot, 'target', 'release', binaryName);

    if (fs.existsSync(debugFromRoot)) {
        console.log('Karu LSP: found debug binary in workspace root');
        return debugFromRoot;
    }
    if (fs.existsSync(releaseFromRoot)) {
        console.log('Karu LSP: found release binary in workspace root');
        return releaseFromRoot;
    }

    // 4. Try to find in workspace (cargo build output)
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (workspaceFolders) {
        for (const folder of workspaceFolders) {
            const releasePath = path.join(folder.uri.fsPath, 'target', 'release', binaryName);
            const debugPath = path.join(folder.uri.fsPath, 'target', 'debug', binaryName);

            if (fs.existsSync(releasePath)) {
                console.log('Karu LSP: found release binary in workspace');
                return releasePath;
            }
            if (fs.existsSync(debugPath)) {
                console.log('Karu LSP: found debug binary in workspace');
                return debugPath;
            }
        }
    }

    // 5. Fall back to PATH
    console.log('Karu LSP: falling back to PATH');
    return hostBinaryName();
}

function activate(context) {
    const serverPath = findServerPath(context.extensionPath);

    console.log(`Karu LSP: using server at ${serverPath}`);

    // Gutter decorations for test pass/fail
    passDecorationType = vscode.window.createTextEditorDecorationType({
        gutterIconPath: path.join(context.extensionPath, 'icons', 'test-pass.svg'),
        gutterIconSize: '80%',
        overviewRulerColor: '#4ec966',
        overviewRulerLane: vscode.OverviewRulerLane.Left,
    });

    failDecorationType = vscode.window.createTextEditorDecorationType({
        gutterIconPath: path.join(context.extensionPath, 'icons', 'test-fail.svg'),
        gutterIconSize: '80%',
        overviewRulerColor: '#f14c4c',
        overviewRulerLane: vscode.OverviewRulerLane.Left,
    });

    // Gutter decorations for rule coverage
    coverageFullDecorationType = vscode.window.createTextEditorDecorationType({
        gutterIconPath: path.join(context.extensionPath, 'icons', 'coverage-full.svg'),
        gutterIconSize: '60%',
    });

    coveragePartialDecorationType = vscode.window.createTextEditorDecorationType({
        gutterIconPath: path.join(context.extensionPath, 'icons', 'coverage-partial.svg'),
        gutterIconSize: '60%',
    });

    coverageNoneDecorationType = vscode.window.createTextEditorDecorationType({
        gutterIconPath: path.join(context.extensionPath, 'icons', 'coverage-none.svg'),
        gutterIconSize: '60%',
    });

    const serverOptions = {
        run: { command: serverPath, transport: TransportKind.stdio },
        debug: { command: serverPath, transport: TransportKind.stdio }
    };

    const clientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'karu' },
            { scheme: 'file', language: 'cedar' },
            { scheme: 'file', language: 'cedarschema' }
        ],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{karu,cedar,cedarschema}')
        }
    };

    client = new LanguageClient(
        'karuLanguageServer',
        'Karu Language Server',
        serverOptions,
        clientOptions
    );

    client.start().then(() => {
        // Listen for test results + coverage (diagnostics handled by LSP directly)
        client.onNotification('karu/testResults', (params) => {
            applyTestDecorations(params.uri, params.tests);
            applyCoverageDecorations(params.uri, params.coverage);
        });
    }).catch(err => {
        vscode.window.showErrorMessage(
            `Failed to start Karu LSP: ${err.message}. ` +
            `Build with 'cargo build --bin karu-lsp' or set 'karu.serverPath'.`
        );
    });
}

/**
 * Apply gutter decorations for test results (✓/✗).
 */
function applyTestDecorations(uriString, results) {
    const uri = vscode.Uri.parse(uriString);
    const editor = vscode.window.visibleTextEditors.find(
        e => e.document.uri.toString() === uri.toString()
    );
    if (!editor) return;

    const passDecorations = [];
    const failDecorations = [];

    for (const result of results) {
        const range = new vscode.Range(result.line, 0, result.line, 0);
        if (result.passed) {
            passDecorations.push({ range });
        } else {
            failDecorations.push({ range });
        }
    }

    editor.setDecorations(passDecorationType, passDecorations);
    editor.setDecorations(failDecorationType, failDecorations);
}

/**
 * Apply gutter dots for rule coverage.
 */
function applyCoverageDecorations(uriString, coverage) {
    const uri = vscode.Uri.parse(uriString);
    const editor = vscode.window.visibleTextEditors.find(
        e => e.document.uri.toString() === uri.toString()
    );
    if (!editor) return;

    const full = [];
    const partial = [];
    const none = [];

    for (const rule of coverage) {
        const range = new vscode.Range(rule.line, 0, rule.line, 0);

        if (rule.status === 'full') {
            full.push({ range });
        } else if (rule.status === 'partial') {
            partial.push({ range });
        } else {
            none.push({ range });
        }
    }

    editor.setDecorations(coverageFullDecorationType, full);
    editor.setDecorations(coveragePartialDecorationType, partial);
    editor.setDecorations(coverageNoneDecorationType, none);
}

function deactivate() {
    if (passDecorationType) passDecorationType.dispose();
    if (failDecorationType) failDecorationType.dispose();
    if (coverageFullDecorationType) coverageFullDecorationType.dispose();
    if (coveragePartialDecorationType) coveragePartialDecorationType.dispose();
    if (coverageNoneDecorationType) coverageNoneDecorationType.dispose();
    if (client) {
        return client.stop();
    }
}

module.exports = { activate, deactivate };
