import { useEffect } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { Toaster } from 'sonner'
import { useAuthStore } from './stores/auth'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Gallery from './pages/Gallery'
import ImageDetail from './pages/ImageDetail'
import ProtectedRoute from './components/ProtectedRoute'

export default function App() {
  const loadFromStorage = useAuthStore((s) => s.loadFromStorage)
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const hasLoaded = useAuthStore((s) => s.hasLoaded)

  useEffect(() => {
    loadFromStorage()
  }, [loadFromStorage])

  return (
    <>
      <Routes>
        <Route
          path="/"
          element={
            !hasLoaded ? (
              <div className="flex min-h-screen items-center justify-center bg-gray-950">
                <div className="h-8 w-8 animate-spin rounded-full border-2 border-gray-600 border-t-white" />
              </div>
            ) : (
              <Navigate to={isAuthenticated ? '/dashboard' : '/login'} replace />
            )
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
          path="/gallery"
          element={
            <ProtectedRoute>
              <Gallery />
            </ProtectedRoute>
          }
        />
        <Route
          path="/images/:id"
          element={
            <ProtectedRoute>
              <ImageDetail />
            </ProtectedRoute>
          }
        />
      </Routes>
      <Toaster position="top-right" richColors />
    </>
  )
}