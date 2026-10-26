/** 与 Rust 端 serde 结构一一对应（字段名保持 snake_case） */

export type StrategyKind = "mask" | "placeholder" | "delete";
export type RuleKind = "regex" | "keyword" | "ner";
export type FindingSource = "regex" | "keyword" | "ner" | "propagated" | "manual" | "model";
export type ViewMode = "original" | "redacted" | "compare";

export interface StrategyConfig {
  kind: StrategyKind;
  mask_char: string;
  keep_prefix: number;
  keep_suffix: number;
  placeholder: string;
}

export interface PageRange {
  page: number;
  start: number;
  end: number;
  label: string;
}

export interface Finding {
  id: string;
  start: number;
  end: number;
  text: string;
  category: string;
  category_label: string;
  confidence: number;
  source: FindingSource;
  rule_id: string | null;
  rule_name: string;
  enabled: boolean;
  strategy: StrategyConfig;
}

export interface DocSummary {
  id: string;
  file_name: string;
  file_path: string;
  kind: string;
  kind_label: string;
  page_count: number;
  char_count: number;
  file_size: number;
  needs_ocr: boolean;
  ocr_used: boolean;
  finding_count: number;
  enabled_count: number;
  detected: boolean;
  warnings: string[];
}

export interface DocPayload {
  id: string;
  file_name: string;
  file_path: string;
  kind: string;
  kind_label: string;
  page_count: number;
  char_count: number;
  file_size: number;
  sha256: string;
  pages: PageRange[];
  text: string;
  findings: Finding[];
  needs_ocr: boolean;
  ocr_used: boolean;
  warnings: string[];
}

export interface Rule {
  id: string;
  name: string;
  category: string;
  category_label: string;
  kind: RuleKind;
  pattern: string;
  keywords: string[];
  capture_group: number;
  confidence: number;
  strategy: StrategyConfig;
  validator: "none" | "id_card" | "luhn" | "uscc";
  priority: number;
  enabled: boolean;
  require_boundary: boolean;
  case_insensitive: boolean;
  builtin: boolean;
  description: string;
}

export interface Settings {
  threshold: number;
  ocr_dpi: number;
  ocr_engine: string;
  external_ocr_command: string;
  pdf_export_dpi: number;
  default_mask_char: string;
  placeholder_template: string;
  propagate_names: boolean;
  enhance_model: boolean;
  last_output_dir: string;
}

export interface CategoryInfo {
  id: string;
  label: string;
}

export interface OcrStatus {
  engine: string;
  engine_label: string;
  ready: boolean;
  message: string;
  models_dir: string;
}

export interface EnhanceStatus {
  ready: boolean;
  message: string;
  model_dir: string;
}

export interface AppPaths {
  config_dir: string;
  rules_file: string;
  settings_file: string;
  models_dir: string;
}

export interface ImportOutcome {
  docs: DocSummary[];
  errors: string[];
}

export interface RedactionSegment {
  start: number;
  end: number;
  replacement: string;
  category: string;
  category_label: string;
  mode: string;
}

export interface RedactionPreview {
  text: string;
  segments: RedactionSegment[];
  total: number;
  by_category: [string, string, number][];
}

export interface ExportedFile {
  path: string;
  label: string;
  bytes: number;
}

export interface ExportOutcome {
  doc_id: string;
  file_name: string;
  files: ExportedFile[];
  audit_path: string;
  warnings: string[];
  total: number;
  error: string | null;
}

export interface RuleTestResult {
  matches: string[];
  count: number;
  error: string | null;
}

export interface ExportRequest {
  doc_ids: string[];
  out_dir: string;
  formats: string[];
  native_only: boolean;
  write_audit: boolean;
}

/** OCR 页级进度（后端通过 ocr-progress 事件推送） */
export interface OcrProgress {
  file_name: string;
  current: number;
  total: number;
  phase: "loading" | "page" | "done" | "error" | string;
}
