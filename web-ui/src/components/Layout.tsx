import { type ReactNode } from 'react'
import NavBar from './NavBar'

interface LayoutProps {
  children: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  return (
    <>
      <NavBar />
      <main
        className="mx-auto max-w-5xl px-4 py-6 sm:px-6"
        style={{ fontFamily: "'Inter', system-ui, sans-serif" }}
      >
        {children}
      </main>
    </>
  )
}
