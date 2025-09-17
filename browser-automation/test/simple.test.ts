/**
 * Simple test to verify Jest setup and test server connectivity
 */

import fetch from 'node-fetch';
import { TEST_CONFIG } from './setup';

describe('Test Setup Validation', () => {
  test('should have test configuration', () => {
    expect(TEST_CONFIG.TEST_SERVER_PORT).toBe(3030);
    expect(TEST_CONFIG.TEST_SERVER_URL).toBe('http://127.0.0.1:3030');
  });
  
  test('should connect to test server', async () => {
    const response = await fetch(`${TEST_CONFIG.TEST_SERVER_URL}/api/status`);
    const data = await response.json() as { status: string; timestamp: string };
    
    expect(response.ok).toBe(true);
    expect(data.status).toBe('ok');
    expect(data.timestamp).toBeDefined();
  });
});