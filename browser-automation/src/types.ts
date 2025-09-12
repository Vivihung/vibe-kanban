export interface BrowserConfig {
  headless: boolean;
  stealth: boolean;
  timeout: number;
  viewport?: {
    width: number;
    height: number;
  };
}

export interface ChatAgent {
  name: string;
  url: string;
  selectors: ChatSelectors;
}

export interface ChatSelectors {
  input: string[];
  sendButton: string[];
  responseContent: string[];
  loginIndicators?: string[];
}

export interface AutomationResult {
  success: boolean;
  response?: string;
  error?: string;
  duration: number;
}

export enum LogLevel {
  INFO = 'info',
  ERROR = 'error',
  DEBUG = 'debug'
}