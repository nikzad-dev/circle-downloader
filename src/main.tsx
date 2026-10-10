import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { AppProviders } from '#components/providers'
import './App.css'

const rootElement = document.getElementById('root')

if (!rootElement) {
  throw new Error('Root element with id "root" was not found')
}

createRoot(rootElement).render(
  <StrictMode>
    <AppProviders />
  </StrictMode>,
)
