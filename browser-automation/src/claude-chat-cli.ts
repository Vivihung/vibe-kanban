#!/usr/bin/env node

/**
 * CLI wrapper for Claude Chat browser automation
 * 
 * This script provides a command-line interface for sending messages to Claude Chat
 * via browser automation. It's compiled from TypeScript to JavaScript and called
 * by the Rust backend when browser chat tasks are executed.
 * 
 * Usage: node claude-chat-cli.js --agent claude --message "Your message here"
 */

import { sendMessageAndGetResponse } from './browser-chat';

// Parse command line arguments
const args = process.argv.slice(2);
let agent = 'claude';
let message = 'Hello';

for (let i = 0; i < args.length; i++) {
  if (args[i] === '--agent' && i + 1 < args.length) {
    agent = args[i + 1];
    i++;
  } else if (args[i] === '--message' && i + 1 < args.length) {
    message = args[i + 1];
    i++;
  }
}

// Run the browser automation
async function main(): Promise<void> {
  console.log(`Starting browser automation for ${agent} with message: ${message}`);
  
  try {
    const response = await sendMessageAndGetResponse(agent, message);
    console.log('Response received:', response);
  } catch (error) {
    console.error('Browser automation failed:', error);
    process.exit(1);
  }
}

main();