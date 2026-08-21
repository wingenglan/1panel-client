import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/editor/editor.worker.js?worker";
import cssWorker from "monaco-editor/language/css/css.worker.js?worker";
import htmlWorker from "monaco-editor/language/html/html.worker.js?worker";
import jsonWorker from "monaco-editor/language/json/json.worker.js?worker";
import tsWorker from "monaco-editor/language/typescript/ts.worker.js?worker";
import { loader } from "@monaco-editor/react";

/** 配置 Monaco 使用随桌面应用打包的 Worker，避免编辑器依赖 CDN 或外网。 */
function configureMonacoWorkers() {
  (globalThis as typeof globalThis & { MonacoEnvironment: { getWorker: (_moduleId: string, label: string) => Worker } }).MonacoEnvironment = {
    /** 根据编辑器语言返回对应的本地 Worker。 */
    getWorker(_moduleId, label) {
      if (label === "json") return new jsonWorker();
      if (label === "css" || label === "scss" || label === "less") return new cssWorker();
      if (label === "html" || label === "handlebars" || label === "razor") return new htmlWorker();
      if (label === "typescript" || label === "javascript") return new tsWorker();
      return new editorWorker();
    },
  };
  loader.config({ monaco });
}

configureMonacoWorkers();
