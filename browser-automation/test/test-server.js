#!/usr/bin/env node

/**
 * Test server that simulates chat interfaces for testing browser automation
 * without requiring real authentication to external services.
 */

const express = require('express');
const path = require('path');
const fs = require('fs');

const app = express();
const PORT = 3030;

// Serve static files
app.use(express.static(path.join(__dirname, 'pages')));
app.use(express.json());

// State management for simulated chat sessions
const chatSessions = new Map();

// Claude Chat simulation
app.get('/claude-chat', (req, res) => {
  const html = fs.readFileSync(path.join(__dirname, 'pages/claude-chat.html'), 'utf8');
  res.send(html);
});

// M365 Copilot simulation
app.get('/m365-chat', (req, res) => {
  const html = fs.readFileSync(path.join(__dirname, 'pages/m365-chat.html'), 'utf8');
  res.send(html);
});

// Login simulation pages
app.get('/claude-login', (req, res) => {
  const html = fs.readFileSync(path.join(__dirname, 'pages/claude-login.html'), 'utf8');
  res.send(html);
});

app.get('/m365-login', (req, res) => {
  const html = fs.readFileSync(path.join(__dirname, 'pages/m365-login.html'), 'utf8');
  res.send(html);
});

// API endpoints for chat simulation
app.post('/api/send-message', (req, res) => {
  const { message, sessionId } = req.body;
  
  // Simulate processing time
  setTimeout(() => {
    const response = `Test response to: "${message}" (Session: ${sessionId || 'new'})`;
    res.json({ 
      response, 
      sessionId: sessionId || 'test-session-' + Date.now(),
      complete: true 
    });
  }, 2000); // 2 second delay to simulate real chat
});

// Endpoint to simulate login completion
app.post('/api/login', (req, res) => {
  const { username } = req.body;
  res.json({ success: true, redirectTo: '/claude-chat' });
});

// Status endpoint for health checks
app.get('/api/status', (req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// Start server
app.listen(PORT, '127.0.0.1', () => {
  console.log(`Test server running at http://127.0.0.1:${PORT}`);
  console.log('Available test pages:');
  console.log(`  - Claude Chat: http://127.0.0.1:${PORT}/claude-chat`);
  console.log(`  - M365 Chat: http://127.0.0.1:${PORT}/m365-chat`);
  console.log(`  - Claude Login: http://127.0.0.1:${PORT}/claude-login`);
  console.log(`  - M365 Login: http://127.0.0.1:${PORT}/m365-login`);
});

module.exports = app;