const fs = require('fs');
const path = require('path');

const workspaceRoot = path.resolve(__dirname, '..', '..', '..', '..');
const artifactRoot = process.env.KARU_VSIX_ARTIFACT_DIR
    ? path.resolve(process.env.KARU_VSIX_ARTIFACT_DIR)
    : null;
const outputRoot = path.join(__dirname, 'bin');

const platforms = [
    {
        bundleDir: 'linux-x64',
        target: 'x86_64-unknown-linux-gnu',
        artifactDir: 'vsix-karu-lsp-x86_64-unknown-linux-gnu',
        binaryName: 'karu-lsp',
    },
    {
        bundleDir: 'linux-arm64',
        target: 'aarch64-unknown-linux-gnu',
        artifactDir: 'vsix-karu-lsp-aarch64-unknown-linux-gnu',
        binaryName: 'karu-lsp',
    },
    {
        bundleDir: 'darwin-x64',
        target: 'x86_64-apple-darwin',
        artifactDir: 'vsix-karu-lsp-x86_64-apple-darwin',
        binaryName: 'karu-lsp',
    },
    {
        bundleDir: 'darwin-arm64',
        target: 'aarch64-apple-darwin',
        artifactDir: 'vsix-karu-lsp-aarch64-apple-darwin',
        binaryName: 'karu-lsp',
    },
    {
        bundleDir: 'win32-x64',
        target: 'x86_64-pc-windows-msvc',
        artifactDir: 'vsix-karu-lsp-x86_64-pc-windows-msvc',
        binaryName: 'karu-lsp.exe',
    },
    {
        bundleDir: 'win32-arm64',
        target: 'aarch64-pc-windows-msvc',
        artifactDir: 'vsix-karu-lsp-aarch64-pc-windows-msvc',
        binaryName: 'karu-lsp.exe',
    },
];

function recreateDirectory(dir) {
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
}

function findSourceBinary(platform) {
    const candidates = [];

    if (artifactRoot) {
        candidates.push(path.join(artifactRoot, platform.artifactDir, platform.binaryName));
    }

    candidates.push(
        path.join(workspaceRoot, 'target', platform.target, 'release', platform.binaryName),
        path.join(workspaceRoot, 'target', 'release', platform.binaryName),
    );

    return candidates.find(candidate => fs.existsSync(candidate));
}

function copyBundledBinary(platform) {
    const source = findSourceBinary(platform);
    if (!source) {
        const searched = [];
        if (artifactRoot) {
            searched.push(path.join(artifactRoot, platform.artifactDir, platform.binaryName));
        }
        searched.push(
            path.join(workspaceRoot, 'target', platform.target, 'release', platform.binaryName),
            path.join(workspaceRoot, 'target', 'release', platform.binaryName),
        );
        throw new Error(
            `Missing ${platform.target} binary. Looked in:\n${searched.map(item => `- ${item}`).join('\n')}`,
        );
    }

    const destinationDir = path.join(outputRoot, platform.bundleDir);
    const destination = path.join(destinationDir, platform.binaryName);
    fs.mkdirSync(destinationDir, { recursive: true });
    fs.copyFileSync(source, destination);

    if (!platform.binaryName.endsWith('.exe')) {
        fs.chmodSync(destination, 0o755);
    }

    console.log(`Bundled ${platform.target} from ${source}`);
}

recreateDirectory(outputRoot);
platforms.forEach(copyBundledBinary);
