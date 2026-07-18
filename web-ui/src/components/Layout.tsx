import { type ReactNode } from 'react'
import NavBar from './NavBar'

interface LayoutProps {
  children: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  return (
    <>
      <NavBar />
      <main className="mx-auto max-w-5xl p-4">
        {children}
      </main>
    </>
  )
}
