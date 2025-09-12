import { ChatAgent } from '../types';

export const m365CopilotAgent: ChatAgent = {
  name: 'M365 Copilot',
  url: 'https://m365.cloud.microsoft.com/chat',
  selectors: {
    input: [
      '[data-testid="chat-input"]',
      'textarea[placeholder*="Ask"]',
      '[role="textbox"]',
      '.chat-input textarea',
      'div[contenteditable="true"]'
    ],
    sendButton: [
      '[data-testid="send-button"]',
      'button[aria-label*="Send"]',
      'button[type="submit"]',
      '.send-button',
      'button:has(svg[data-icon="send"])'
    ],
    responseContent: [
      '[data-testid="copilot-response"]:last-child',
      '.response-message:last-child',
      '.chat-message:last-child .content',
      '[role="article"]:last-child',
      '.message-content:last-child'
    ],
    loginIndicators: [
      '.sign-in-button',
      'button:contains("Sign in")',
      '[data-testid="sign-in"]',
      'input[type="email"]',
      '.microsoft-login'
    ]
  }
};