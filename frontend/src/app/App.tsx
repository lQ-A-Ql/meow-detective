import { RouterProvider } from 'react-router';
import { router } from './routes';
import '../styles/fonts.css';

export default function App() {
  return <RouterProvider router={router} />;
}
