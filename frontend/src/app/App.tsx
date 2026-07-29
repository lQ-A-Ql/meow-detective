import { RouterProvider } from 'react-router';
import { ErrorBoundary } from './components/ErrorBoundary';
import { router } from './routes';
import '../styles/fonts.css';

export default function App() {
  return (
    <ErrorBoundary>
      <div
        className="contents"
        onContextMenu={(event) => event.preventDefault()}
      >
        <RouterProvider router={router} />
      </div>
    </ErrorBoundary>
  );
}
