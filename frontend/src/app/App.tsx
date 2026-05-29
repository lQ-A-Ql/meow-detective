import { RouterProvider } from 'react-router';
import { ErrorBoundary } from './components/ErrorBoundary';
import { router } from './routes';
import '../styles/fonts.css';

export default function App() {
  return (
    <ErrorBoundary>
      <RouterProvider router={router} />
    </ErrorBoundary>
  );
}
