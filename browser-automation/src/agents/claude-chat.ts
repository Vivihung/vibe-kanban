import { ChatAgent } from '../types';

export const claudeChatAgent: ChatAgent = {
  name: 'Claude Chat',
  url: 'https://claude.ai/',
  selectors: {
    input: [
      'div[contenteditable="true"][role="textbox"][aria-label*="Claude"]',
      '.ProseMirror[contenteditable="true"]',
      '[role="textbox"][contenteditable="true"]',
      'div[contenteditable="true"].ProseMirror',
      '[data-testid="chat-input"]',
      '[contenteditable="true"]',
      'textarea[placeholder*="message"]',
      'div[contenteditable="true"]'
    ],
    sendButton: [
      'button[aria-label="Send message"]',
      'button:has(svg[viewBox="0 0 256 256"]) svg path[d*="M208.49,120.49"]',
      '[data-testid="send-button"]',
      'button[type="submit"]',
      '[aria-label*="Send"]',
      'button:has(svg[data-icon="send"])',
      'button:has(svg[viewBox*="24"])'
    ],
    responseContent: [
      '[data-testid="message-content"]:last-child',
      '.message:last-child',
      '[role="article"]:last-child',
      '.prose:last-child',
      '[data-message-author="assistant"]:last-child'
    ],
    loginIndicators: [
      '[data-testid="login-form"]',
      'button:contains("Log in")',
      'button:contains("Sign in")',
      'input[type="email"]',
      '.login-form'
    ]
  }
};