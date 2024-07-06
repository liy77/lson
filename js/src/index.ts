import { SpawnSyncReturns, spawnSync } from "node:child_process"
import { existsSync } from "node:fs";
import { join as joinPath } from "node:path";
const WORKING_PATH = process.cwd();

let BIN_PATH: string;

if (process.platform === "win32") {
    BIN_PATH = joinPath(WORKING_PATH, "win32", "lson.exe");
} else if (process.platform === "linux") {
    BIN_PATH = joinPath(WORKING_PATH, "linux", "lson");
} else if (process.platform === "darwin") {
    // TODO: Add support for darwin
    throw new Error("Darwin is not supported yet"); 
    BIN_PATH = joinPath(WORKING_PATH, "darwin", "lson"); // This is just a placeholder
}

function parse(content: string): string;
function parse(file: string) {
    const args = ["raw", "compile", "-t", "json"];

    if (!existsSync(file)) {
        args.push("--text", file);
    } else {
        args.push("-f", file);
    }

    const r = spawnSync(BIN_PATH, args, {
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
    const args = ["raw", "compile", "-t", "lson"];

    if (!existsSync(file)) {
        args.push("--text", file);
    } else {
        args.push("-f", file);
    }

    const r = spawnSync(BIN_PATH, args, {
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