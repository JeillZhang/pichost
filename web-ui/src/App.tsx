import { Routes, Route, Navigate } from 'react-router-dom'
import { Toaster } from 'sonner'

export default function App() {
  return (
    <>
      <Routes>
        <Route path="/" element={<Navigate to="/login" replace />} />
        <Route path="/login" element={<div className="p-8 text-center text-gray-400">Login — coming soon</div>} />
        <Route path="/dashboard" element={<div className="p-8 text-center text-gray-400">Dashboard — coming soon</div>} />
        <Route path="/images/:id" element={<div className="p-8 text-center text-gray-400">Image Detail — coming soon</div>} />
      </Routes>
      <Toaster position="top-right" richColors />
    </>
  )
}