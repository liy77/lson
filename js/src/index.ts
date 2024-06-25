import { SpawnSyncReturns, spawnSync } from "node:child_process"
import { existsSync } from "node:fs";
import { join as joinPath } from "node:path";
const WORKING_PATH = process.cwd();

function parse(content: string): string;
function parse(file: string) {
    const path = joinPath(WORKING_PATH, "/bin/lson");
    const args = ["raw", "compile", "-t", "json"];

    if (!existsSync(file)) {
        args.push("--text", file);
    } else {
        args.push("-f", file);
    }

    const r = spawnSync(path, ["raw", "compile", "-t", "json"], {
        stdio: "pipe"
    });

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
    const args = ["raw", "compile", "-t", "lson"];

    if (!existsSync(file)) {
        args.push("--text", file);
    } else {
        args.push("-f", file);
    }

    const r = spawnSync(path, args, {
        stdio: "pipe"
    });

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