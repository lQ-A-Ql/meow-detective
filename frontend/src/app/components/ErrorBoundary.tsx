import { Component, type ErrorInfo, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
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
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="min-h-screen flex items-center justify-center bg-white">
          <div className="w-full max-w-lg border border-[#e0e0e0] bg-[#fafafa] p-8 text-center">
            <div className="font-serif text-2xl text-[#111] mb-3">应用发生错误</div>
            <div className="text-[13px] text-[#666] leading-6 mb-4 font-mono break-all">
              {this.state.error?.message ?? '未知错误'}
            </div>
            <div className="flex gap-3 justify-center">
              <button
                type="button"
                onClick={this.handleReload}
                className="border border-[#ccc] bg-white text-[#111] hover:bg-[#f0f0f0] px-4 py-2 text-[12px] rounded-[2px] cursor-pointer font-medium"
              >
                重新加载
              </button>
              <button
                type="button"
                onClick={this.handleReset}
                className="border border-transparent text-[#666] hover:text-[#111] px-4 py-2 text-[12px] cursor-pointer underline hover:no-underline"
              >
                尝试恢复
              </button>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
