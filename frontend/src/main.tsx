import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './app/App';
import { AppProviders } from './app/providers';
import { startTauriEventBridge } from './lib/events/tauri-bridge';
import './i18n';
import './styles/index.css';

startTauriEventBridge();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AppProviders>
      <App />
    </AppProviders>
  </React.StrictMode>,
);
