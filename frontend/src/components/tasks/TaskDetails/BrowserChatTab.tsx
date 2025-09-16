import { useState, useEffect } from 'react';
import { Send, MessageSquare, AlertCircle, CheckCircle, Clock } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useAttemptExecution } from '@/hooks';
import type { TaskAttempt, ExecutionProcess } from 'shared/types';

interface BrowserChatTabProps {
  selectedAttempt: TaskAttempt | null;
}

interface BrowserChatMessage {
  id: string;
  message: string;
  agentType: 'claude' | 'm365';
  timestamp: string;
  status: 'pending' | 'completed' | 'failed';
  response?: string;
  executionProcessId?: string;
}

function BrowserChatTab({ selectedAttempt }: BrowserChatTabProps) {
  const { attemptData } = useAttemptExecution(selectedAttempt?.id);
  const [message, setMessage] = useState('');
  const [selectedAgent, setSelectedAgent] = useState<'claude' | 'm365'>('claude');
  const [isHealthy, setIsHealthy] = useState<boolean | null>(null);
  const [isSending, setIsSending] = useState(false);
  const [chatHistory, setChatHistory] = useState<BrowserChatMessage[]>([]);

  // Check browser automation health on component mount
  useEffect(() => {
    checkHealth();
  }, []);

  // Filter browser chat processes from execution processes
  const browserChatProcesses = (attemptData.processes || []).filter(
    (process: ExecutionProcess) => process.run_reason === 'browserchat'
  );

  const checkHealth = async () => {
    try {
      const response = await fetch('/api/browser-chat/health');
      const result = await response.json();
      setIsHealthy(result.data.healthy);
    } catch (error) {
      console.error('Failed to check browser chat health:', error);
      setIsHealthy(false);
    }
  };

  const sendMessage = async () => {
    if (!message.trim() || !selectedAttempt?.id || isSending) return;

    setIsSending(true);
    const newMessage: BrowserChatMessage = {
      id: Date.now().toString(),
      message: message.trim(),
      agentType: selectedAgent,
      timestamp: new Date().toISOString(),
      status: 'pending',
    };

    setChatHistory(prev => [...prev, newMessage]);
    setMessage('');

    try {
      const response = await fetch(`/api/browser-chat/task-attempts/${selectedAttempt.id}/send`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          message: newMessage.message,
          agent_type: selectedAgent === 'claude' ? 'claude' : 'm365',
          executor_profile_id: 'CLAUDE_BROWSER_CHAT', // Default for now
        }),
      });

      const result = await response.json();
      
      if (result.success) {
        setChatHistory(prev => 
          prev.map(msg => 
            msg.id === newMessage.id 
              ? { 
                  ...msg, 
                  status: 'completed', 
                  executionProcessId: result.data.execution_process_id,
                  response: result.data.message 
                }
              : msg
          )
        );
      } else {
        setChatHistory(prev => 
          prev.map(msg => 
            msg.id === newMessage.id 
              ? { ...msg, status: 'failed', response: result.error || 'Unknown error' }
              : msg
          )
        );
      }
    } catch (error) {
      console.error('Failed to send browser chat message:', error);
      setChatHistory(prev => 
        prev.map(msg => 
          msg.id === newMessage.id 
            ? { ...msg, status: 'failed', response: 'Network error' }
            : msg
        )
      );
    } finally {
      setIsSending(false);
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'pending':
        return <Clock className="h-4 w-4 text-blue-500 animate-spin" />;
      case 'completed':
        return <CheckCircle className="h-4 w-4 text-green-500" />;
      case 'failed':
        return <AlertCircle className="h-4 w-4 text-destructive" />;
      default:
        return <Clock className="h-4 w-4 text-gray-400" />;
    }
  };

  if (isHealthy === false) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            Browser automation is not available. Please check your configuration and ensure the browser automation scripts are properly set up.
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="w-full h-full flex flex-col space-y-4">
      {/* Health Status */}
      {isHealthy !== null && (
        <Alert variant={isHealthy ? "default" : "destructive"}>
          <MessageSquare className="h-4 w-4" />
          <AlertDescription>
            Browser automation is {isHealthy ? 'ready' : 'not available'}
          </AlertDescription>
        </Alert>
      )}

      {/* Chat History */}
      <div className="flex-1 overflow-y-auto space-y-4">
        {chatHistory.length === 0 ? (
          <div className="flex-1 flex items-center justify-center text-muted-foreground">
            <div className="text-center">
              <MessageSquare className="h-12 w-12 mx-auto mb-4 opacity-50" />
              <p>No browser chat messages yet. Send a message to get started.</p>
            </div>
          </div>
        ) : (
          chatHistory.map((msg) => (
            <Card key={msg.id} className="w-full">
              <CardHeader className="pb-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <Badge variant={msg.agentType === 'claude' ? 'default' : 'secondary'}>
                      {msg.agentType === 'claude' ? 'Claude Chat' : 'M365 Copilot'}
                    </Badge>
                    {getStatusIcon(msg.status)}
                  </div>
                  <span className="text-xs text-muted-foreground">
                    {new Date(msg.timestamp).toLocaleTimeString()}
                  </span>
                </div>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <p className="text-sm font-medium">Message:</p>
                    <p className="text-sm text-muted-foreground">{msg.message}</p>
                  </div>
                  {msg.response && (
                    <div>
                      <p className="text-sm font-medium">
                        {msg.status === 'failed' ? 'Error:' : 'Response:'}
                      </p>
                      <p className={`text-sm ${msg.status === 'failed' ? 'text-destructive' : 'text-muted-foreground'}`}>
                        {msg.response}
                      </p>
                    </div>
                  )}
                  {msg.executionProcessId && (
                    <div className="text-xs text-muted-foreground">
                      Process ID: {msg.executionProcessId}
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>
          ))
        )}

        {/* Show browser chat processes from execution history */}
        {browserChatProcesses.length > 0 && (
          <div className="space-y-2">
            <h3 className="text-sm font-medium">Browser Chat Processes</h3>
            {browserChatProcesses.map((process) => (
              <Card key={process.id} className="w-full">
                <CardContent className="pt-4">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      {getStatusIcon(process.status)}
                      <span className="text-sm">Browser Chat Process</span>
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {process.started_at && new Date(process.started_at).toLocaleString()}
                    </div>
                  </div>
                  {process.exit_code !== null && Number(process.exit_code) !== 0 && (
                    <div className="mt-2 text-sm text-destructive">
                      Exit code: {String(process.exit_code)}
                    </div>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </div>

      {/* Message Input */}
      <div className="border-t pt-4">
        <div className="space-y-4">
          {/* Agent Selection */}
          <div className="flex space-x-2">
            <Button
              variant={selectedAgent === 'claude' ? 'default' : 'outline'}
              size="sm"
              onClick={() => setSelectedAgent('claude')}
            >
              Claude Chat
            </Button>
            <Button
              variant={selectedAgent === 'm365' ? 'default' : 'outline'}
              size="sm"
              onClick={() => setSelectedAgent('m365')}
            >
              M365 Copilot
            </Button>
          </div>

          {/* Message Input */}
          <div className="flex space-x-2">
            <Textarea
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder="Enter your message for the browser-based chat agent..."
              className="flex-1 min-h-[80px]"
              disabled={!isHealthy || isSending}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                  e.preventDefault();
                  sendMessage();
                }
              }}
            />
            <Button
              onClick={sendMessage}
              disabled={!message.trim() || !isHealthy || isSending || !selectedAttempt}
              size="sm"
              className="self-end"
            >
              <Send className="h-4 w-4" />
              Send
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            Press Ctrl+Enter to send • Browser automation must be configured
          </p>
        </div>
      </div>
    </div>
  );
}

export default BrowserChatTab;