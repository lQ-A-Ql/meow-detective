import { Component, type ErrorInfo, type ReactNode } from 'react';
import { withTranslation, type WithTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';

interface Props extends WithTranslation {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

class ErrorBoundaryBase extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    console.error('[ErrorBoundary]', error, errorInfo);
  }

  private handleReload = (): void => {
    window.location.reload();
  };

  private handleReset = (): void => {
    this.setState({ hasError: false, error: null });
  };

  render(): ReactNode {
    const { t, children, fallback } = this.props;

    if (this.state.hasError) {
      if (fallback) {
        return fallback;
      }

      return (
        <div className="min-h-screen flex items-center justify-center bg-forensics-surface">
          <div className="w-full max-w-lg border border-forensics-border bg-forensics-panel p-8 text-center">
            <div className="font-serif text-2xl text-forensics-text mb-3">{t('errorBoundary.title')}</div>
            <div className="text-[13px] text-forensics-muted leading-6 mb-4 font-mono break-all">
              {this.state.error?.message ?? t('errorBoundary.unknownError')}
            </div>
            <div className="flex gap-3 justify-center">
              <Button
                type="button"
                variant="forensicsSurface"
                size="sm"
                onClick={this.handleReload}
              >
                {t('errorBoundary.reload')}
              </Button>
              <Button
                type="button"
                variant="forensicsLink"
                size="sm"
                onClick={this.handleReset}
              >
                {t('errorBoundary.recover')}
              </Button>
            </div>
          </div>
        </div>
      );
    }

    return children;
  }
}

export const ErrorBoundary = withTranslation()(ErrorBoundaryBase);
