/**
 * Component files, such as `App.svelte` and `App.vue`: a file of markup that holds its code
 * in `<script>` blocks.
 *
 * The compiler reads TypeScript only. So each component file is shown to it as a file with
 * `.ts` after its name, `App.svelte.ts`, whose text is that of the component with a space in
 * place of each character outside the scripts. The code is then at the line and the column
 * that it has in the component, and an import of `./App.svelte` leads to the file.
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

/** The extensions of the component files that are read. */
export const COMPONENT_EXTENSIONS: readonly ComponentExtension[] = [".svelte", ".vue", ".astro"];

/** The extension of a kind of component file that refpact reads. */
export type ComponentExtension = ".svelte" | ".vue" | ".astro";

/** What the compiler asks of the file system. `undefined` lets the compiler read the disk. */
export interface ComponentFiles {
  readFile(fileName: string): string | undefined;
  fileExists(fileName: string): boolean | undefined;
  getAccessibleEntries(directoryName: string): { files: string[]; directories: string[] } | undefined;
}

/** The types of the scripts that are code of the component. Other scripts, such as JSON, are data. */
const CODE_TYPES = new Set(["", "module", "text/javascript", "text/typescript", "ts", "js", "typescript", "javascript"]);

/**
 * The text of a component file with a space in place of each character that is not code: all
 * that is outside its `<script>` blocks, and outside the part between `---` lines at the
 * start of an Astro file. Line breaks stay.
 */
export function scriptsOf(text: string, extension: ComponentExtension): string {
  const keep: [number, number][] = [];
  if (extension === ".astro") {
    const front = /^\s*---\r?\n([\s\S]*?)\r?\n---/.exec(text);
    if (front !== null) {
      const start = front.index + front[0].indexOf(front[1] as string, front[0].indexOf("---") + 3);
      keep.push([start, start + (front[1] as string).length]);
    }
  }
  const block = /<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi;
  for (let found = block.exec(text); found !== null; found = block.exec(text)) {
    const attributes = found[1] as string;
    const type = /\btype\s*=\s*["']?([^"'\s>]*)/i.exec(attributes)?.[1] ?? "";
    if (/\bsrc\s*=/i.test(attributes) || !CODE_TYPES.has(type.toLowerCase())) {
      continue;
    }
    const start = found.index + found[0].indexOf(">") + 1;
    keep.push([start, start + (found[2] as string).length]);
  }
  let code = "";
  let at = 0;
  for (const [start, end] of keep.sort((a, b) => a[0] - b[0])) {
    if (start < at) {
      continue;
    }
    code += text.slice(at, start).replace(/[^\r\n]/g, " ") + text.slice(start, end);
    at = end;
  }
  return code + text.slice(at).replace(/[^\r\n]/g, " ");
}

/** The component file that a file name of the compiler stands for: `App.svelte` for `App.svelte.ts`. */
export function componentOf(fileName: string, extensions: readonly ComponentExtension[]): { readonly file: string; readonly extension: ComponentExtension } | undefined {
  if (!fileName.endsWith(".ts")) {
    return undefined;
  }
  const file = fileName.slice(0, -".ts".length);
  const extension = extensions.find((known) => file.endsWith(known));
  return extension === undefined ? undefined : { file, extension };
}

/**
 * The file system that shows the component files to the compiler. A file that is on the
 * disk under the name, such as a module `state.svelte.ts` of Svelte, is read from the disk.
 */
export function componentFiles(extensions: readonly ComponentExtension[]): ComponentFiles {
  const isFile = (file: string): boolean => {
    try {
      return statSync(file).isFile();
    } catch {
      return false;
    }
  };
  const shown = (fileName: string): { readonly file: string; readonly extension: ComponentExtension } | undefined => {
    const component = componentOf(fileName, extensions);
    return component !== undefined && !existsSync(fileName) && isFile(component.file) ? component : undefined;
  };
  return {
    readFile(fileName) {
      const component = shown(fileName);
      return component === undefined ? undefined : scriptsOf(readFileSync(component.file, "utf8"), component.extension);
    },
    fileExists: (fileName) => (shown(fileName) !== undefined ? true : undefined),
    getAccessibleEntries(directoryName) {
      let names: string[];
      try {
        names = readdirSync(directoryName);
      } catch {
        return undefined;
      }
      if (!names.some((name) => extensions.some((extension) => name.endsWith(extension)))) {
        return undefined;
      }
      const files: string[] = [];
      const directories: string[] = [];
      for (const name of names) {
        const entry = path.join(directoryName, name);
        if (isFile(entry)) {
          files.push(name);
          if (extensions.some((extension) => name.endsWith(extension)) && !names.includes(`${name}.ts`)) {
            files.push(`${name}.ts`);
          }
        } else if (existsSync(entry)) {
          directories.push(name);
        }
      }
      return { files, directories };
    },
  };
}
