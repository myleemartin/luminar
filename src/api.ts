// Tauri 命令封装：前端所有后端调用集中在这里，便于审计「无网络请求」。
import { invoke } from "@tauri-apps/api/core";
import type {
  AppPaths,
  CategoryInfo,
  DocPayload,
  DocSummary,
  ExportOutcome,
  ExportRequest,
  Finding,
  ImportOutcome,
  EnhanceStatus,
  OcrStatus,
  RedactionPreview,
  Rule,
  RuleTestResult,
  Settings,
  StrategyConfig,
} from "./types";

export const api = {
  appPaths: () => invoke<AppPaths>("app_paths"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  listRules: () => invoke<Rule[]>("list_rules"),
  saveRule: (rule: Rule) => invoke<Rule[]>("save_rule", { rule }),
  deleteRule: (id: string) => invoke<Rule[]>("delete_rule", { id }),
  resetRules: () => invoke<Rule[]>("reset_rules"),
  listCategories: () => invoke<CategoryInfo[]>("list_categories"),
  testRule: (rule: Rule, text: string) => invoke<RuleTestResult>("test_rule", { rule, text }),
  ocrStatus: () => invoke<OcrStatus>("ocr_status"),
  enhanceStatus: () => invoke<EnhanceStatus>("enhance_status"),

  importDocuments: (paths: string[]) => invoke<ImportOutcome>("import_documents", { paths }),
  getDocument: (id: string) => invoke<DocPayload>("get_document", { id }),
  detectDocument: (id: string) => invoke<Finding[]>("detect_document", { id }),
  redetectAll: () => invoke<DocSummary[]>("redetect_all"),
  reparseDocument: (id: string) => invoke<Finding[]>("reparse_document", { id }),
  removeDocument: (id: string) => invoke<void>("remove_document", { id }),

  updateFinding: (docId: string, finding: Finding) =>
    invoke<Finding[]>("update_finding", { docId, finding }),
  addManualFinding: (params: {
    docId: string;
    start: number;
    end: number;
    category: string;
    categoryLabel: string;
    strategy: StrategyConfig;
  }) => invoke<Finding[]>("add_manual_finding", params),
  removeFinding: (docId: string, findingId: string) =>
    invoke<Finding[]>("remove_finding", { docId, findingId }),
  setAllEnabled: (docId: string, enabled: boolean) =>
    invoke<Finding[]>("set_all_enabled", { docId, enabled }),
  redactPreview: (docId: string) => invoke<RedactionPreview>("redact_preview", { docId }),

  exportDocuments: (req: ExportRequest) => invoke<ExportOutcome[]>("export_documents", { req }),
  defaultExportDir: () => invoke<string>("default_export_dir"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
};
