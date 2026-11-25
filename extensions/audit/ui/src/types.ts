export type AuditSeverity = "error" | "warning" | "notice";

export interface AuditIssue {
  code: string;
  severity: AuditSeverity;
  title: string;
  detail: string;
}

export interface AuditMetadata {
  title: string | null;
  description: string | null;
  canonicalUrl: string | null;
  robots: string | null;
  language: string | null;
  heading: string | null;
  openGraphTitle: string | null;
  openGraphDescription: string | null;
  openGraphImageUrl: string | null;
  openGraphType: string | null;
  twitterCard: string | null;
  twitterTitle: string | null;
  twitterDescription: string | null;
  twitterImageUrl: string | null;
  wordCount: number;
}

export interface AuditPage {
  url: string;
  statusCode: number | null;
  responseMillis: number | null;
  bytes: number;
  metadata: AuditMetadata;
  issues: AuditIssue[];
  error: string | null;
}

export interface AuditReport {
  site: string;
  startedAt: string;
  pages: AuditPage[];
  errors: number;
  warnings: number;
  notices: number;
  byCode: [string, AuditSeverity, number][];
}
