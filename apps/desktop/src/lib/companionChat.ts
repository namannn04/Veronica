export interface ModelConfig {
  provider: string;
  endpoint: string;
  model: string;
  managedLocal: boolean;
  allowRemote: boolean;
  modelDirectory: string;
}
export interface ChatMessage {
  id: string;
  role: string;
  text: string;
  createdAt: string;
}
export interface ChatView {
  enabled: boolean;
  pending: boolean;
  dataPath: string;
  totalMessages: number;
  document: { config: ModelConfig; memory: string; messages: ChatMessage[] };
}
export const DEFAULT_MODEL: ModelConfig = {
  provider: "ollama",
  endpoint: "http://127.0.0.1:11434",
  model: "llama3.2:latest",
  managedLocal: true,
  allowRemote: false,
  modelDirectory: "",
};
