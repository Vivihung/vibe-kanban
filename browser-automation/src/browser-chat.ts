#!/usr/bin/env node

import puppeteer from 'puppeteer-extra';
import StealthPlugin from 'puppeteer-extra-plugin-stealth';
import { Page, Browser } from 'puppeteer';
import { logger } from './utils/logger';
import * as path from 'path';
import * as os from 'os';

// Configure Puppeteer with stealth plugin
puppeteer.use(StealthPlugin());

interface AgentConfig {
  name: string;
  url: string;
  inputSelectors: string[];
  sendButtonSelectors: string[];
}

const agents: Record<string, AgentConfig> = {
  'claude': {
    name: 'Claude Chat',
    url: 'https://claude.ai/chat',
    inputSelectors: [
      '[contenteditable="true"]',
      'textarea[placeholder*="message"]',
      'textarea[placeholder*="Message"]', 
      'div[contenteditable="true"]',
      '[data-testid="chat-input"]',
      '.ProseMirror',
      'textarea'
    ],
    sendButtonSelectors: [
      'button[type="submit"]',
      'button[aria-label*="Send"]',
      '[data-testid="send-button"]'
    ]
  },
  'm365': {
    name: 'M365 Copilot',
    url: 'https://m365.cloud.microsoft.com/chat',
    inputSelectors: [
      'textarea[placeholder*="Ask"]',
      'textarea[placeholder*="message"]',
      '[contenteditable="true"]',
      '[data-testid="chat-input"]',
      'div[contenteditable="true"]',
      'textarea'
    ],
    sendButtonSelectors: [
      'button[type="submit"]',
      'button[aria-label*="Send"]',
      '[data-testid="send-button"]',
      '.send-button'
    ]
  }
};

async function checkIfLoggedIn(page: Page, agent: AgentConfig): Promise<boolean> {
  try {
    // Check for login indicators (sign that user is NOT logged in)
    const loginIndicators = [
      'button:contains("Log in")',
      'button:contains("Sign in")',
      'input[type="email"]',
      '[data-testid="login-form"]',
      '.login-form',
      'a[href*="login"]',
      'a[href*="signin"]'
    ];

    for (const selector of loginIndicators) {
      try {
        await page.waitForSelector(selector, { timeout: 2000 });
        logger.info(`Found login indicator: ${selector}`);
        return false; // Login form found, not logged in
      } catch (e) {
        continue;
      }
    }

    // Check for chat interface (sign that user IS logged in)
    for (const selector of agent.inputSelectors) {
      try {
        await page.waitForSelector(selector, { timeout: 5000 });
        logger.info(`Found chat input: ${selector}`);
        return true; // Chat interface found, logged in
      } catch (e) {
        continue;
      }
    }

    // If no clear indicators, assume not logged in for safety
    return false;
  } catch (error) {
    logger.debug('Error checking login status:', error);
    return false;
  }
}

async function sendMessageToAgent(agentName: string, message: string): Promise<void> {
  const agent = agents[agentName.toLowerCase()];
  if (!agent) {
    throw new Error(`Unknown agent: ${agentName}. Available: ${Object.keys(agents).join(', ')}`);
  }

  let browser: Browser | null = null;

  try {
    logger.info(`Initializing ${agent.name} automation...`);
    
    // Create persistent user data directory for this agent
    const userDataDir = path.join(os.homedir(), '.browser-automation', `${agentName.toLowerCase()}-profile`);
    
    browser = await puppeteer.launch({
      headless: false, // Always visible for manual login
      userDataDir, // Enable session persistence
      args: [
        '--no-sandbox',
        '--disable-setuid-sandbox',
        '--disable-dev-shm-usage',
        '--disable-accelerated-2d-canvas',
        '--no-first-run',
        '--no-zygote',
        '--disable-gpu'
      ]
    });

    const page: Page = await browser.newPage();

    // Set viewport and user agent to match your working code
    await page.setViewport({ width: 1366, height: 768 });
    await page.setUserAgent(
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36'
    );

    // Navigate to the agent
    logger.info(`Navigating to ${agent.name} at ${agent.url}...`);
    await page.goto(agent.url, {
      waitUntil: 'networkidle2',
      timeout: 120000 // 2 minutes
    });

    // Wait for page to load
    logger.info('Waiting for page to load...');
    await new Promise(resolve => setTimeout(resolve, 5000));

    // Check if already logged in by looking for chat interface
    const isLoggedIn = await checkIfLoggedIn(page, agent);
    
    if (!isLoggedIn) {
      // Manual login prompt
      logger.info('Please login manually in the browser window.');
      logger.info('Press Enter in this terminal when ready to continue...');
      
      await new Promise((resolve) => {
        process.stdin.once('data', resolve);
      });
    } else {
      logger.info('Already logged in, continuing with automation...');
    }

    logger.info('Continuing with automation...');

    // Find message input using multiple selectors
    logger.info('Looking for message input...');
    let messageInput = null;

    for (const selector of agent.inputSelectors) {
      try {
        await page.waitForSelector(selector, { timeout: 5000 });
        messageInput = await page.$(selector);
        if (messageInput) {
          logger.info(`Found input with selector: ${selector}`);
          break;
        }
      } catch (e) {
        logger.debug(`Selector ${selector} not found, trying next...`);
        continue;
      }
    }

    if (!messageInput) {
      logger.error('Could not find message input. Taking screenshot for debugging...');
      await page.screenshot({ path: 'debug.png' });
      throw new Error('Message input not found');
    }

    // Type the message
    logger.info(`Typing message: "${message}"`);
    await messageInput.click();
    await messageInput.type(message);

    // Send the message
    logger.info('Sending message...');
    
    // Try Enter key first (like your working code)
    await page.keyboard.press('Enter');

    // Wait and try button click as backup
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    for (const selector of agent.sendButtonSelectors) {
      try {
        const sendButton = await page.$(selector);
        if (sendButton) {
          logger.debug(`Found send button with selector: ${selector}`);
          await sendButton.click();
          break;
        }
      } catch (e) {
        logger.debug(`Send button selector ${selector} failed`);
        continue;
      }
    }

    // Wait for response and stream output
    logger.info('Waiting for response...');
    await streamResponse(page, agent);

    logger.info(`Successfully sent message to ${agent.name}`);

  } catch (error) {
    logger.error('Error:', error);
    throw error;
  } finally {
    if (browser) {
      logger.info('Closing browser...');
      await browser.close();
    }
  }
}

async function streamResponse(page: Page, agent: AgentConfig): Promise<void> {
  // Simple response streaming - look for last message content
  const responseSelectors = [
    '[data-testid="message-content"]:last-child',
    '.message:last-child .content',
    '[role="article"]:last-child',
    '.prose:last-child',
    '[data-message-author="assistant"]:last-child'
  ];

  let lastResponse = '';
  let attempts = 0;
  const maxAttempts = 60; // 30 seconds

  while (attempts < maxAttempts) {
    try {
      let currentResponse = '';

      for (const selector of responseSelectors) {
        try {
          const element = await page.$(selector);
          if (element) {
            currentResponse = await page.evaluate((el: Element) => 
              el.textContent || (el as HTMLElement).innerText || '', element);
            if (currentResponse && currentResponse.trim()) {
              break;
            }
          }
        } catch (e) {
          continue;
        }
      }

      if (currentResponse && currentResponse !== lastResponse) {
        const newContent = currentResponse.substring(lastResponse.length);
        if (newContent.trim()) {
          process.stdout.write(newContent);
        }
        lastResponse = currentResponse;
      }

      // Simple completion check
      if (currentResponse && (
        currentResponse.endsWith('.') || 
        currentResponse.endsWith('!') || 
        currentResponse.endsWith('?')
      )) {
        // Wait a bit more to ensure completion
        await new Promise(resolve => setTimeout(resolve, 2000));
        break;
      }

      await new Promise(resolve => setTimeout(resolve, 500));
      attempts++;

    } catch (error) {
      logger.debug('Error in response streaming:', error);
      attempts++;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
  }

  if (attempts >= maxAttempts) {
    logger.info('\nResponse streaming timeout');
  } else {
    logger.info('\nResponse streaming complete');
  }
}

// CLI Interface
async function main(): Promise<void> {
  const args = process.argv.slice(2);
  
  if (args.length < 2) {
    console.error('Usage: npm run dev <agent> "<message>"');
    console.error('Agents: claude, m365');
    console.error('Example: npm run dev claude "Hello, can you help me?"');
    process.exit(1);
  }

  const [agentName, ...messageArgs] = args;
  const message = messageArgs.join(' ');

  // Handle process termination gracefully
  process.on('SIGINT', () => {
    logger.info('Received SIGINT, exiting...');
    process.exit(0);
  });

  process.on('SIGTERM', () => {
    logger.info('Received SIGTERM, exiting...');
    process.exit(0);
  });

  try {
    await sendMessageToAgent(agentName, message);
  } catch (error) {
    logger.error('Fatal error:', error);
    process.exit(1);
  }
}

// Only run main if this file is executed directly
if (require.main === module) {
  main().catch(error => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}

export { sendMessageToAgent };