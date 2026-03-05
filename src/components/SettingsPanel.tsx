import React, { useState, useEffect } from 'react';
import { getAvailableModels, saveLlmConfig, getNarrative } from '../api';
import './SettingsPanel.css';

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

interface ProviderOption {
  value: string;
  label: string;
  defaultBaseUrl: string;
}

const PROVIDERS: ProviderOption[] = [
  { value: 'lmstudio', label: 'LM Studio', defaultBaseUrl: 'http://localhost:1234' },
  { value: 'ollama', label: 'Ollama', defaultBaseUrl: 'http://localhost:11434' },
  { value: 'openai', label: 'OpenAI', defaultBaseUrl: 'https://api.openai.com' },
  { value: 'anthropic', label: 'Anthropic', defaultBaseUrl: 'https://api.anthropic.com' },
  { value: 'deepseek', label: 'DeepSeek', defaultBaseUrl: 'https://api.deepseek.com' },
  { value: 'nanogpt', label: 'NanoGPT', defaultBaseUrl: 'https://nano-gpt.com/api/v1' },
];

export const SettingsPanel: React.FC<SettingsPanelProps> = ({ isOpen, onClose }) => {
  const [provider, setProvider] = useState<string>('lmstudio');
  const [baseUrl, setBaseUrl] = useState<string>('http://localhost:1234');
  const [apiKey, setApiKey] = useState<string>('');
  const [model, setModel] = useState<string>('');
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [isLoadingModels, setIsLoadingModels] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [showApiKey, setShowApiKey] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // Load saved config on mount (from localStorage or defaults)
  useEffect(() => {
    const savedConfig = localStorage.getItem('engine13_llm_config');
    if (savedConfig) {
      try {
        const config = JSON.parse(savedConfig);
        setProvider(config.provider || 'lmstudio');
        setBaseUrl(config.base_url || 'http://localhost:1234');
        setApiKey(config.api_key || '');
        setModel(config.model || '');
      } catch (e) {
        console.error('Failed to load saved config:', e);
      }
    }
  }, []);

  // Update base URL when provider changes
  useEffect(() => {
    const selectedProvider = PROVIDERS.find(p => p.value === provider);
    if (selectedProvider) {
      setBaseUrl(selectedProvider.defaultBaseUrl);
    }
  }, [provider]);

  const handleLoadModels = async () => {
    setIsLoadingModels(true);
    setMessage(null);
    try {
      const models = await getAvailableModels();
      setAvailableModels(models);
      setMessage({ type: 'success', text: `Loaded ${models.length} models` });
    } catch (err) {
      setMessage({ type: 'error', text: `Failed to load models: ${err}` });
    } finally {
      setIsLoadingModels(false);
    }
  };

  const handleSave = async () => {
    setIsSaving(true);
    setMessage(null);
    try {
      await saveLlmConfig(provider, baseUrl, apiKey || null, model);
      // Save to localStorage for persistence across sessions
      localStorage.setItem('engine13_llm_config', JSON.stringify({
        provider,
        base_url: baseUrl,
        api_key: apiKey,
        model,
      }));
      setMessage({ type: 'success', text: 'Configuration saved successfully' });
    } catch (err) {
      setMessage({ type: 'error', text: `Failed to save: ${err}` });
    } finally {
      setIsSaving(false);
    }
  };

  const handleTest = async () => {
    setIsTesting(true);
    setTestResult(null);
    setMessage(null);
    try {
      const result = await getNarrative();
      setTestResult(result);
      setMessage({ type: 'success', text: 'Test successful' });
    } catch (err) {
      setMessage({ type: 'error', text: `Test failed: ${err}` });
    } finally {
      setIsTesting(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-modal" onClick={e => e.stopPropagation()}>
        <div className="settings-header">
          <h2 className="settings-title">LLM Settings</h2>
          <button className="settings-close" onClick={onClose}>×</button>
        </div>

        <div className="settings-content">
          {message && (
            <div className={`settings-message ${message.type}`}>
              {message.text}
            </div>
          )}

          <div className="settings-form">
            <div className="form-group">
              <label className="form-label">Provider</label>
              <select
                className="form-select"
                value={provider}
                onChange={e => setProvider(e.target.value)}
              >
                {PROVIDERS.map(p => (
                  <option key={p.value} value={p.value}>{p.label}</option>
                ))}
              </select>
            </div>

            <div className="form-group">
              <label className="form-label">Base URL</label>
              <input
                type="text"
                className="form-input"
                value={baseUrl}
                onChange={e => setBaseUrl(e.target.value)}
                placeholder="https://api.example.com"
              />
            </div>

            <div className="form-group">
              <label className="form-label">API Key</label>
              <div className="password-input-wrapper">
                <input
                  type={showApiKey ? 'text' : 'password'}
                  className="form-input"
                  value={apiKey}
                  onChange={e => setApiKey(e.target.value)}
                  placeholder="Enter your API key"
                />
                <button
                  type="button"
                  className="password-toggle"
                  onClick={() => setShowApiKey(!showApiKey)}
                >
                  {showApiKey ? '🙈' : '👁'}
                </button>
              </div>
            </div>

            <div className="form-group">
              <label className="form-label">Model</label>
              <div className="model-input-wrapper">
                <input
                  type="text"
                  className="form-input"
                  value={model}
                  onChange={e => setModel(e.target.value)}
                  placeholder="e.g., gpt-4, claude-3, llama-2"
                />
                {availableModels.length > 0 && (
                  <select
                    className="form-select model-select"
                    value={model}
                    onChange={e => setModel(e.target.value)}
                  >
                    <option value="">Select a model...</option>
                    {availableModels.map(m => (
                      <option key={m} value={m}>{m}</option>
                    ))}
                  </select>
                )}
              </div>
            </div>

            <div className="form-actions">
              <button
                className="btn btn-secondary"
                onClick={handleLoadModels}
                disabled={isLoadingModels}
              >
                {isLoadingModels ? 'Loading...' : 'Load Models'}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleTest}
                disabled={isTesting || !model}
              >
                {isTesting ? 'Testing...' : 'Test'}
              </button>
              <button
                className="btn btn-success"
                onClick={handleSave}
                disabled={isSaving}
              >
                {isSaving ? 'Saving...' : 'Save'}
              </button>
            </div>

            {testResult && (
              <div className="test-result">
                <h4 className="test-result-title">Test Result:</h4>
                <p className="test-result-text">{testResult}</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default SettingsPanel;
