import { useEffect } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { Toaster } from 'sonner'
import { useAuthStore } from './stores/auth'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import ProtectedRoute from './components/ProtectedRoute'

export default function App() {
  const loadFromStorage = useAuthStore((s) => s.loadFromStorage)
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)

  useEffect(() => {
    loadFromStorage()
  }, [loadFromStorage])

  return (
    <>
      <Routes>
        <Route
          path="/"
          element={
            <Navigate to={isAuthenticated ? '/dashboard' : '/login'} replace />
          }
        />
        <Route path="/login" element={<Login />} />
        <Route
          path="/dashboard"
          element={
            <ProtectedRoute>
              <Dashboard />
            </ProtectedRoute>
          }
        />
        <Route
          path="/images/:id"
          element={
            <ProtectedRoute>
              <div className="p-8 text-center text-gray-400">
                Image Detail — coming soon
              </div>
            </ProtectedRoute>
          }
        />
      </Routes>
      <Toaster position="top-right" richColors />
    </>
  )
}