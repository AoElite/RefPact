import { spawn } from "node:child_process";

export function run(command: string): number | undefined {
  return spawn(command).pid;
}
