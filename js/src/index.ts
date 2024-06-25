import { SpawnSyncReturns, spawnSync } from "node:child_process"
import { existsSync } from "node:fs";
import { join as joinPath } from "node:path";
const WORKING_PATH = process.cwd();

function parse(content: string): string;
function parse(file: string) {
    const path = joinPath(WORKING_PATH, "/bin/lson");
    let r: SpawnSyncReturns<Buffer> | undefined;

    if (!existsSync(file)) {
        r = spawnSync(path, ["raw", "compile", "--text", file, "-t", "json"], {
            stdio: "pipe"
        });
    } else {
        r = spawnSync(path, ["raw", "compile", "-f", file, "-t", "json"], {
            stdio: "pipe"
        });
    }

    if (r.status !== 0) {
        console.error(r.stderr.toString());
        process.exit(1);
    }

    const json = JSON.parse(r.stdout.toString());
    return json;
}

function compile(content: string): string;
function compile(file: string) {
    const path = joinPath(WORKING_PATH, "/bin/lson");
    let r: SpawnSyncReturns<Buffer> | undefined;

    if (!existsSync(file)) {
        r = spawnSync(path, ["raw", "compile", "--text", file, "-t", "lson"], {
            stdio: "pipe"
        });
    } else {
        r = spawnSync(path, ["raw", "compile", "-f", file, "-t", "lson"], {
            stdio: "pipe"
        });
    }

    if (r.status !== 0) {
        console.error(r.stderr.toString());
        process.exit(1);
    }

    const lson = r.stdout.toString();
    return lson;
}

export {
    compile,
    parse
}