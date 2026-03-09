import React, { useState, useEffect, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { Sidebar } from './components/Layout/Sidebar';
import { Header } from './components/Layout/Header';

import { appLogger } from './lib/logger';
import { isTauri } from './lib/tauri';
import { Download, X, Loader2, CheckCircle, AlertCircle, Megaphone, ExternalLink } from 'lucide-react';

// Lazy loaded page components
const Dashboard = React.lazy(() => import('./components/Dashboard').then(module => ({ default: module.Dashboard })));
const AIConfig = React.lazy(() => import('./components/AIConfig').then(module => ({ default: module.AIConfig })));
const Channels = React.lazy(() => import('./components/Channels').then(module => ({ default: module.Channels })));
const MCP = React.lazy(() => import('./components/MCP').then(module => ({ default: module.MCP })));
const Skills = React.lazy(() => import('./components/Skills').then(module => ({ default: module.Skills })));
const Settings = React.lazy(() => import('./components/Settings').then(module => ({ default: module.Settings })));
const Logs = React.lazy(() => import('./components/Logs').then(module => ({ default: module.Logs })));
const Agents = React.lazy(() => import('./components/Agents').then(module => ({ default: module.Agents })));

export type PageType = 'dashboard' | 'mcp' | 'skills' | 'ai' | 'channels' | 'agents' | 'logs' | 'settings';

export interface EnvironmentStatus {
  node_installed: boolean;
  node_version: string | null;
  node_version_ok: boolean;
  git_installed: boolean;
  git_version: string | null;
  openclaw_installed: boolean;
  openclaw_version: string | null;
  gateway_service_installed: boolean;
  config_dir_exists: boolean;
  ready: boolean;
  os: string;
}

interface ServiceStatus {
  running: boolean;
  pid: number | null;
  port: number;
}

interface UpdateInfo {
  update_available: boolean;
  current_version: string | null;
  latest_version: string | null;
  error: string | null;
}

interface UpdateResult {
  success: boolean;
  message: string;
  error?: string;
}

interface SecureVersionInfo {
  current_version: string;
  is_secure: boolean;
}

interface AnnouncementInfo {
  id?: string;
  enabled?: boolean;
  title?: string;
  message?: string;
  action_text?: string | null;
  action_url?: string | null;
  starts_at?: string | null;
  ends_at?: string | null;
}

const ANNOUNCEMENT_URL = 'https://www.steadyai.work/announcement.json';
const ANNOUNCEMENT_DISMISSED_KEY = 'openclaw_manager_dismissed_announcement';
const ANNOUNCEMENT_REFRESH_MS = 60 * 1000;

type AnnouncementStatus = 'unknown' | 'active' | 'inactive';

class ErrorBoundary extends React.Component<{ children: React.ReactNode }, { hasError: boolean, error: Error | null }> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    appLogger.error('ErrorBoundary caught error', { error, errorInfo });
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="p-8 text-center">
          <AlertCircle size={48} className="mx-auto text-red-400 mb-4" />
          <h2 className="text-xl font-bold text-white mb-2">出现了一点问题</h2>
          <p className="text-red-200 mb-4">{this.state.error?.message}</p>
          <button
            onClick={() => this.setState({ hasError: false })}
            className="px-4 py-2 bg-dark-700 hover:bg-dark-600 rounded-lg text-white text-sm"
          >
            重试
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}

function App() {
  const [currentPage, setCurrentPage] = useState<PageType>('dashboard');
  const [envStatus, setEnvStatus] = useState<EnvironmentStatus | null>(null);
  const [serviceStatus, setServiceStatus] = useState<ServiceStatus | null>(null);

  // Update related state
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [showUpdateBanner, setShowUpdateBanner] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updateResult, setUpdateResult] = useState<UpdateResult | null>(null);

  // Manager Update state
  const [managerUpdateAvailable, setManagerUpdateAvailable] = useState(false);
  const [managerUpdateVersion, setManagerUpdateVersion] = useState<string | null>(null);
  const [showManagerUpdateBanner, setShowManagerUpdateBanner] = useState(false);
  const [managerUpdating, setManagerUpdating] = useState(false);
  const [managerUpdateProgress, setManagerUpdateProgress] = useState(0);
  const [managerUpdateResult, setManagerUpdateResult] = useState<UpdateResult | null>(null);
  const [managerUpdateObj, setManagerUpdateObj] = useState<any>(null);

  // Security check state
  const [secureVersionInfo, setSecureVersionInfo] = useState<SecureVersionInfo | null>(null);
  const [showSecurityBanner, setShowSecurityBanner] = useState(false);

  // Announcement state
  const [announcementInfo, setAnnouncementInfo] = useState<AnnouncementInfo | null>(null);
  const [showAnnouncementBanner, setShowAnnouncementBanner] = useState(false);
  const [announcementStatus, setAnnouncementStatus] = useState<AnnouncementStatus>('unknown');

  // Check environment
  const checkEnvironment = useCallback(async () => {
    if (!isTauri()) {
      appLogger.warn('Not in Tauri environment, skipping environment check');
      return;
    }

    appLogger.info('Starting system environment check...');
    try {
      const status = await invoke<EnvironmentStatus>('check_environment');
      appLogger.info('Environment check completed', status);
      setEnvStatus(status);
    } catch (e) {
      appLogger.error('Environment check failed', e);
    }
  }, []);

  // Check for updates
  const checkUpdate = useCallback(async () => {
    if (!isTauri()) return;

    appLogger.info('Checking for OpenClaw updates...');
    try {
      const info = await invoke<UpdateInfo>('check_openclaw_update');
      appLogger.info('Update check result', info);
      setUpdateInfo(info);
      if (info.update_available) {
        setShowUpdateBanner(true);
      }
    } catch (e) {
      appLogger.error('Update check failed', e);
    }
  }, []);

  // Check Manager Update
  const checkManagerUpdate = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (update) {
        setManagerUpdateAvailable(true);
        setManagerUpdateVersion(update.version);
        setManagerUpdateObj(update);
        setShowManagerUpdateBanner(true);
      }
    } catch (e) {
      appLogger.error('Manager update check failed', e);
    }
  }, []);

  // Check security version
  const checkSecurity = useCallback(async () => {
    if (!isTauri()) return;

    appLogger.info('Checking OpenClaw version security...');
    try {
      const info = await invoke<SecureVersionInfo>('check_secure_version');
      appLogger.info('Security check result', info);
      setSecureVersionInfo(info);
      if (!info.is_secure) {
        setShowSecurityBanner(true);
      }
    } catch (e) {
      appLogger.error('Security check failed', e);
    }
  }, []);

  const checkAnnouncement = useCallback(async () => {
    try {
      const response = await fetch(`${ANNOUNCEMENT_URL}?t=${Date.now()}`, {
        cache: 'no-store',
      });

      if (!response.ok) {
        setAnnouncementStatus('inactive');
        setAnnouncementInfo(null);
        setShowAnnouncementBanner(false);
        return;
      }

      const info = await response.json() as AnnouncementInfo;
      if (!info?.enabled || (!info.title && !info.message)) {
        setAnnouncementStatus('inactive');
        setAnnouncementInfo(null);
        setShowAnnouncementBanner(false);
        return;
      }

      const now = Date.now();
      const startsAt = info.starts_at ? Date.parse(info.starts_at) : null;
      const endsAt = info.ends_at ? Date.parse(info.ends_at) : null;

      if (startsAt && !Number.isNaN(startsAt) && now < startsAt) {
        setAnnouncementStatus('inactive');
        setAnnouncementInfo(null);
        setShowAnnouncementBanner(false);
        return;
      }

      if (endsAt && !Number.isNaN(endsAt) && now > endsAt) {
        setAnnouncementStatus('inactive');
        setAnnouncementInfo(null);
        setShowAnnouncementBanner(false);
        return;
      }

      const dismissed = info.id && localStorage.getItem(ANNOUNCEMENT_DISMISSED_KEY) === info.id;
      setAnnouncementStatus('active');
      setAnnouncementInfo(info);
      setShowAnnouncementBanner(!dismissed);
    } catch (e) {
      appLogger.warn('Announcement check failed', e);
    }
  }, []);

  // Perform update
  const handleUpdate = async () => {
    setUpdating(true);
    setUpdateResult(null);
    try {
      const result = await invoke<UpdateResult>('update_openclaw');
      setUpdateResult(result);
      if (result.success) {
        // Re-check environment after successful update
        await checkEnvironment();
        // Close notification after 3 seconds
        setTimeout(() => {
          setShowUpdateBanner(false);
          setUpdateResult(null);
        }, 3000);
      }
    } catch (e) {
      setUpdateResult({
        success: false,
        message: '更新过程中出现错误',
        error: String(e),
      });
    } finally {
      setUpdating(false);
    }
  };

  // Perform Manager Update (from banner)
  const handleManagerUpdate = async () => {
    if (!managerUpdateObj) return;
    setManagerUpdating(true);
    setManagerUpdateProgress(0);
    setManagerUpdateResult(null);
    try {
      let downloaded = 0;
      let contentLength = 1;
      await managerUpdateObj.downloadAndInstall((event: any) => {
        switch (event.event) {
          case 'Started':
            contentLength = event.data.contentLength || 1;
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            setManagerUpdateProgress(Math.min(100, Math.round((downloaded / contentLength) * 100)));
            break;
          case 'Finished':
            setManagerUpdateProgress(100);
            break;
        }
      });
      setManagerUpdateResult({ success: true, message: '更新安装成功，正在重启...' });

      // Restart app after 2 seconds
      setTimeout(async () => {
        try {
          const { relaunch } = await import('@tauri-apps/plugin-process');
          await relaunch();
        } catch (err) {
          appLogger.error('Relaunch failed', err);
        }
      }, 2000);
    } catch (e: any) {
      appLogger.error('Manager update download failed', e);
      setManagerUpdateResult({ success: false, message: '更新失败', error: e?.message || String(e) });
      setManagerUpdating(false);
    }
  };

  const handleAnnouncementAction = useCallback(async () => {
    if (!announcementInfo?.action_url) return;

    try {
      if (isTauri()) {
        const { open } = await import('@tauri-apps/plugin-shell');
        await open(announcementInfo.action_url);
      } else {
        window.open(announcementInfo.action_url, '_blank', 'noopener,noreferrer');
      }
    } catch (e) {
      appLogger.error('Failed to open announcement link', e);
      window.open(announcementInfo.action_url, '_blank', 'noopener,noreferrer');
    }
  }, [announcementInfo]);

  const dismissAnnouncement = useCallback(() => {
    if (announcementInfo?.id) {
      localStorage.setItem(ANNOUNCEMENT_DISMISSED_KEY, announcementInfo.id);
    }
    setShowAnnouncementBanner(false);
  }, [announcementInfo]);

  useEffect(() => {
    if (announcementStatus !== 'active') return;

    setShowUpdateBanner(false);
    setShowManagerUpdateBanner(false);
    setUpdateResult(null);
    setManagerUpdateResult(null);
  }, [announcementStatus]);

  useEffect(() => {
    appLogger.info('🦞 App component mounted');
    checkEnvironment();
  }, [checkEnvironment]);

  // Check announcements first, then keep polling for new messages.
  useEffect(() => {
    if (!isTauri()) return;
    const timer = setTimeout(() => { checkAnnouncement(); }, 1000);
    const interval = setInterval(() => { checkAnnouncement(); }, ANNOUNCEMENT_REFRESH_MS);
    return () => {
      clearTimeout(timer);
      clearInterval(interval);
    };
  }, [checkAnnouncement]);

  // Only show update banners when no active announcement is being pushed.
  useEffect(() => {
    if (!isTauri() || announcementStatus !== 'inactive') return;

    const timer1 = setTimeout(() => { checkUpdate(); }, 1000);
    const timer2 = setTimeout(() => { checkManagerUpdate(); }, 5000);
    return () => {
      clearTimeout(timer1);
      clearTimeout(timer2);
    };
  }, [announcementStatus, checkUpdate, checkManagerUpdate]);

  // Check security after startup
  useEffect(() => {
    if (!isTauri()) return;
    const timer = setTimeout(() => {
      checkSecurity();
    }, 1000); // Check shortly after startup
    return () => clearTimeout(timer);
  }, [checkSecurity]);

  // Periodically get service status
  useEffect(() => {
    // Don't poll if not in Tauri environment
    if (!isTauri()) return;

    const fetchServiceStatus = async () => {
      try {
        const status = await invoke<ServiceStatus>('get_service_status');
        setServiceStatus(status);
      } catch {
        // Silently handle polling errors
      }
    };
    fetchServiceStatus();
    const interval = setInterval(fetchServiceStatus, 3000);
    return () => clearInterval(interval);
  }, []);

  const handleSetupComplete = useCallback(() => {
    appLogger.info('Setup wizard completed');
    checkEnvironment(); // Re-check environment
  }, [checkEnvironment]);

  // Page navigation handler
  const handleNavigate = (page: PageType) => {
    appLogger.action('Page navigation', { from: currentPage, to: page });
    setCurrentPage(page);
  };

  const renderPage = () => {
    const pageVariants = {
      initial: { opacity: 0, x: 20 },
      animate: { opacity: 1, x: 0 },
      exit: { opacity: 0, x: -20 },
    };

    const pages: Record<PageType, JSX.Element> = {
      dashboard: <Dashboard envStatus={envStatus} onSetupComplete={handleSetupComplete} />,
      mcp: <MCP />,
      skills: <Skills />,
      ai: <AIConfig />,
      channels: <Channels />,
      agents: <Agents />,

      logs: <Logs />,
      settings: <Settings onEnvironmentChange={checkEnvironment} />,
    };

    return (
      <AnimatePresence mode="wait">
        <motion.div
          key={currentPage}
          variants={pageVariants}
          initial="initial"
          animate="animate"
          exit="exit"
          transition={{ duration: 0.2 }}
          className="h-full"
        >
          {pages[currentPage]}
        </motion.div>
      </AnimatePresence>
    );
  };

  const LoadingSpinner = () => (
    <div className="flex h-full items-center justify-center">
      <div className="relative z-10 text-center">
        <div className="inline-flex items-center justify-center w-16 h-16 rounded-xl bg-gradient-to-br from-brand-500 to-purple-600 mb-4 animate-pulse shadow-lg shadow-purple-900/20">
          <span className="text-3xl">🦞</span>
        </div>
        <p className="text-dark-400 font-medium">页面加载中...</p>
      </div>
    </div>
  );

  // Main interface
  return (
    <div className="flex h-screen bg-dark-900 overflow-hidden">
      {/* Background decoration */}
      <div className="fixed inset-0 bg-gradient-radial pointer-events-none" />

      {/* Security Banner (High Priority) */}
      <AnimatePresence>
        {showSecurityBanner && secureVersionInfo && !secureVersionInfo.is_secure && (
          <motion.div
            initial={{ opacity: 0, y: -50 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -50 }}
            className="fixed top-0 left-0 right-0 z-[60] bg-gradient-to-r from-red-600 to-orange-600 shadow-lg"
          >
            <div className="max-w-4xl mx-auto px-4 py-3 flex items-center justify-between">
              <div className="flex items-center gap-3">
                <AlertCircle size={20} className="text-white" />
                <div>
                  <p className="text-sm font-bold text-white">
                    安全警告：你当前的 OpenClaw 版本（{secureVersionInfo.current_version}）存在风险。
                  </p>
                  <p className="text-xs text-white/90">
                    需要升级到 `2026.1.29` 或更高版本，请尽快更新。
                  </p>
                </div>
              </div>
              <button
                onClick={() => setShowSecurityBanner(false)}
                className="p-1.5 hover:bg-white/20 rounded-lg transition-colors text-white/90 hover:text-white"
              >
                <X size={16} />
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Update banner */}
      <AnimatePresence>
        {showUpdateBanner && updateInfo?.update_available && (
          <motion.div
            initial={{ opacity: 0, y: -50 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -50 }}
            className="fixed top-0 left-0 right-0 z-50 bg-gradient-to-r from-claw-600 to-purple-600 shadow-lg"
          >
            <div className="max-w-4xl mx-auto px-4 py-3 flex items-center justify-between">
              <div className="flex items-center gap-3">
                {updateResult?.success ? (
                  <CheckCircle size={20} className="text-green-300" />
                ) : updateResult && !updateResult.success ? (
                  <AlertCircle size={20} className="text-red-300" />
                ) : (
                  <Download size={20} className="text-white" />
                )}
                <div>
                  {updateResult ? (
                    <p className={`text-sm font-medium ${updateResult.success ? 'text-green-100' : 'text-red-100'}`}>
                      {updateResult.message}
                    </p>
                  ) : (
                    <>
                      <p className="text-sm font-medium text-white">
                        发现 OpenClaw 新版本：{updateInfo.latest_version}
                      </p>
                      <p className="text-xs text-white/70">
                        当前版本：{updateInfo.current_version}
                      </p>
                    </>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-2">
                {!updateResult && (
                  <button
                    onClick={handleUpdate}
                    disabled={updating}
                    className="px-4 py-1.5 bg-white/20 hover:bg-white/30 text-white text-sm font-medium rounded-lg transition-colors flex items-center gap-2 disabled:opacity-50"
                  >
                    {updating ? (
                      <>
                        <Loader2 size={14} className="animate-spin" />
                        更新中...
                      </>
                    ) : (
                      <>
                        <Download size={14} />
                        立即更新
                      </>
                    )}
                  </button>
                )}
                <button
                  onClick={() => {
                    setShowUpdateBanner(false);
                    setUpdateResult(null);
                  }}
                  className="p-1.5 hover:bg-white/20 rounded-lg transition-colors text-white/70 hover:text-white"
                >
                  <X size={16} />
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Manager update banner */}
      <AnimatePresence>
        {showManagerUpdateBanner && managerUpdateAvailable && (
          <motion.div
            initial={{ opacity: 0, y: -50 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -50 }}
            className="fixed top-0 left-0 right-0 z-[45] bg-gradient-to-r from-emerald-600 to-teal-600 shadow-lg"
          >
            <div className="max-w-4xl mx-auto px-4 py-3 flex items-center justify-between">
              <div className="flex items-center gap-3 w-1/2">
                {managerUpdateResult?.success ? (
                  <CheckCircle size={20} className="text-green-300 shrink-0" />
                ) : managerUpdateResult && !managerUpdateResult.success ? (
                  <AlertCircle size={20} className="text-red-300 shrink-0" />
                ) : (
                  <Download size={20} className="text-white shrink-0" />
                )}
                <div className="flex-1">
                  {managerUpdateResult ? (
                    <p className={`text-sm font-medium ${managerUpdateResult.success ? 'text-green-100' : 'text-red-100'}`}>
                      {managerUpdateResult.message}
                    </p>
                  ) : (
                    <>
                      <div className="flex justify-between items-center pr-4">
                        <p className="text-sm font-medium text-white">
                          发现管理器新版本：v{managerUpdateVersion}
                        </p>
                        {managerUpdating && (
                          <span className="text-xs text-white/80">{managerUpdateProgress}%</span>
                        )}
                      </div>
                      {managerUpdating && (
                        <div className="w-full bg-black/20 rounded-full h-1 mt-1.5 mr-4 max-w-[200px]">
                          <div
                            className="bg-white h-1 rounded-full transition-all duration-300"
                            style={{ width: `${managerUpdateProgress}%` }}
                          />
                        </div>
                      )}
                    </>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-2">
                {!managerUpdateResult && (
                  <button
                    onClick={handleManagerUpdate}
                    disabled={managerUpdating}
                    className="px-4 py-1.5 bg-white/20 hover:bg-white/30 text-white text-sm font-medium rounded-lg transition-colors flex items-center gap-2 disabled:opacity-50"
                  >
                    {managerUpdating ? (
                      <>
                        <Loader2 size={14} className="animate-spin" />
                        更新中...
                      </>
                    ) : (
                      <>
                        <Download size={14} />
                        立即更新
                      </>
                    )}
                  </button>
                )}
                <button
                  onClick={() => {
                    setShowManagerUpdateBanner(false);
                    setManagerUpdateResult(null);
                  }}
                  className="p-1.5 hover:bg-white/20 rounded-lg transition-colors text-white/70 hover:text-white"
                >
                  <X size={16} />
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Announcement banner */}
      <AnimatePresence>
        {showAnnouncementBanner && announcementInfo && (
          <motion.div
            initial={{ opacity: 0, y: -50 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -50 }}
            className="fixed top-0 left-0 right-0 z-[40] bg-gradient-to-r from-amber-600 to-orange-600 shadow-lg"
          >
            <div className="max-w-4xl mx-auto px-4 py-3 flex items-center justify-between gap-4">
              <div className="flex items-center gap-3 min-w-0">
                <Megaphone size={20} className="text-white shrink-0" />
                <div className="min-w-0">
                  {announcementInfo.title && (
                    <p className="text-sm font-medium text-white truncate">
                      {announcementInfo.title}
                    </p>
                  )}
                  {announcementInfo.message && (
                    <p className="text-xs text-white/85 whitespace-pre-line break-words">
                      {announcementInfo.message}
                    </p>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                {announcementInfo.action_url && (
                  <button
                    onClick={handleAnnouncementAction}
                    className="px-4 py-1.5 bg-white/20 hover:bg-white/30 text-white text-sm font-medium rounded-lg transition-colors flex items-center gap-2"
                  >
                    <ExternalLink size={14} />
                    {announcementInfo.action_text || '查看详情'}
                  </button>
                )}
                <button
                  onClick={dismissAnnouncement}
                  className="p-1.5 hover:bg-white/20 rounded-lg transition-colors text-white/70 hover:text-white"
                >
                  <X size={16} />
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Sidebar */}
      <Sidebar currentPage={currentPage} onNavigate={handleNavigate} serviceStatus={serviceStatus} />

      {/* Main content area */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Header (macOS drag area) */}
        <Header currentPage={currentPage} />

        {/* Page content */}
        <main className="flex-1 overflow-hidden p-6 relative">
          <ErrorBoundary>
            <React.Suspense fallback={<LoadingSpinner />}>
              {renderPage()}
            </React.Suspense>
          </ErrorBoundary>
        </main>
      </div>
    </div>
  );
}

export default App;
