import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { downloadsKey, mergeTask } from '@/features/downloads/api/downloads'
import { routeTree } from '@/routeTree.gen'
import { task } from '@/test/fixtures'
import { ThemeProvider } from './theme/theme-provider'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
async function setup(path = '/') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [path] }),
    context: { queryClient: client },
  })
  await router.load()
  const view = render(
    <ThemeProvider>
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </ThemeProvider>,
  )
  return { client, router, view, user: userEvent.setup() }
}
beforeEach(() => {
  invoke.mockReset()
  localStorage.clear()
  // jsdom has no layout: keep the resize hit target away from pointer clicks at (0, 0).
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(
    function (this: HTMLElement) {
      return this.getAttribute('role') === 'separator'
        ? new DOMRect(224, 48, 1, 720)
        : new DOMRect(0, 0, 1024, 768)
    },
  )
})
describe('shell and shared pages', () => {
  it('uses one menubar for pages and a resizable category sidebar', async () => {
    invoke.mockResolvedValue([])
    const { view, user } = await setup('/downloads/missing')
    expect(
      screen.getAllByRole('menubar', { name: 'Application menu' }),
    ).toHaveLength(1)
    expect(
      screen.queryByRole('navigation', { name: 'Main navigation' }),
    ).not.toBeInTheDocument()
    expect(view.container.querySelector('aside')).toBeInTheDocument()
    expect(
      screen.getByRole('separator', { name: 'Resize download sidebar' }),
    ).toBeInTheDocument()
    await user.click(screen.getByRole('menuitem', { name: 'Pages' }))
    expect(
      await screen.findByRole('menuitem', { name: 'Downloads' }),
    ).toHaveAttribute('aria-current', 'page')
    expect(
      screen.getByRole('menuitem', { name: 'Overview' }),
    ).not.toHaveAttribute('aria-current')
  })
  it('opens Downloads at startup and keeps other pages in the menu', async () => {
    invoke.mockResolvedValue([task()])
    const { router, user } = await setup('/')
    await screen.findByRole('table')
    expect(router.state.location.pathname).toBe('/downloads')
    await user.click(screen.getByRole('menuitem', { name: 'Pages' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'Settings' })
    expect(
      screen.queryByRole('complementary', { name: 'Download filters' }),
    ).not.toBeInTheDocument()
  })
  it('opens a separate download window from the header without a dialog', async () => {
    invoke.mockResolvedValue([])
    const { user } = await setup('/downloads')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(invoke).toHaveBeenCalledWith('open_download_window', {
      initialUrl: '',
      bulk: false,
    })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })
  it('persists appearance choices from application options', async () => {
    invoke.mockResolvedValue([])
    const { user } = await setup()
    await user.click(screen.getByRole('menuitem', { name: 'Appearance' }))
    expect(
      await screen.findByRole('menuitemradio', { name: 'System' }),
    ).toHaveAttribute('aria-checked', 'true')
    await user.click(screen.getByRole('menuitemradio', { name: 'Dark' }))
    await waitFor(() => expect(document.documentElement).toHaveClass('dark'))
    expect(localStorage.getItem('vite-ui-theme')).toBe('dark')
    await user.click(screen.getByRole('menuitem', { name: 'Appearance' }))
    expect(
      await screen.findByRole('menuitemradio', { name: 'Dark' }),
    ).toHaveAttribute('aria-checked', 'true')
    await user.keyboard('{Escape}')
    await user.click(screen.getByRole('menuitem', { name: 'Pages' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Downloads' }))
    expect(
      await screen.findByRole('heading', { name: 'Downloads' }),
    ).toBeInTheDocument()
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
  })
  it('supports keyboard options navigation and Escape focus return', async () => {
    invoke.mockResolvedValue([])
    const { user } = await setup()
    const options = screen.getByRole('menuitem', { name: 'Appearance' })
    options.focus()
    await user.keyboard('{ArrowDown}')
    expect(
      await screen.findByRole('menuitemradio', { name: 'System' }),
    ).toBeInTheDocument()
    await user.keyboard('{Escape}')
    await waitFor(() => expect(options).toHaveFocus())
  })
  it('filters downloads through sidebar categories and keeps the query', async () => {
    invoke.mockResolvedValue([
      task(),
      task({ id: 'paused', status: 'paused', fileName: 'paused.zip' }),
    ])
    const { user, router } = await setup('/downloads?q=zip')
    const categories = await screen.findByRole('navigation', {
      name: 'Download categories',
    })
    await user.click(within(categories).getByRole('link', { name: /Paused/ }))
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({
        q: 'zip',
        status: 'paused',
      }),
    )
    expect(
      within(categories).getByRole('link', { name: /Paused/ }),
    ).toHaveAttribute('aria-current', 'page')
    expect(
      within(await screen.findByRole('table')).getByText('paused.zip'),
    ).toBeInTheDocument()
    expect(
      within(screen.getByRole('table')).queryByText('file.zip'),
    ).not.toBeInTheDocument()
  })
  it('keeps status filters visible and highlights the selected filter', async () => {
    invoke.mockResolvedValue([task({ status: 'completed' })])
    await setup('/downloads?status=completed')
    const categories = screen.getByRole('navigation', {
      name: 'Download categories',
    })
    expect(
      within(categories).getByRole('link', { name: /Completed/ }),
    ).toHaveAttribute('aria-current', 'page')
    expect(
      within(categories).getByRole('link', { name: /Downloading/ }),
    ).toBeInTheDocument()
    expect(
      within(categories).getByRole('link', { name: /All downloads/ }),
    ).not.toHaveAttribute('aria-current')
  })
  it('renders real home summaries and shared navigation', async () => {
    invoke.mockResolvedValue([
      task(),
      task({ id: 'done', status: 'completed', fileName: 'done.zip' }),
      task({
        id: 'bad',
        status: 'failed',
        fileName: 'bad.zip',
        error: {
          code: 'NETWORK_TIMEOUT',
          message: 'timed out',
          retryable: true,
        },
      }),
    ])
    const { user, client } = await setup('/overview')
    expect(await screen.findByText('Active transfers')).toBeInTheDocument()
    expect(screen.getByText('Needs attention')).toBeInTheDocument()
    expect(await screen.findAllByText('bad.zip')).toHaveLength(2)
    await user.click(screen.getByRole('menuitem', { name: 'Pages' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Downloads' }))
    expect(
      await screen.findByRole('heading', { name: 'Downloads' }),
    ).toBeInTheDocument()
    expect(screen.getByText('3 files')).toBeInTheDocument()
    expect(screen.getAllByRole('navigation')[0]).toBeInTheDocument()
    expect(client.getQueryData(downloadsKey)).toHaveLength(3)
  })
  it('keeps global Add usable on empty Downloads and Settings', async () => {
    invoke.mockResolvedValue([])
    const { user } = await setup('/downloads')
    await screen.findByText('Your next download starts here')
    await user.click(
      screen.getByRole('button', { name: 'Add your first download' }),
    )
    expect(invoke).toHaveBeenCalledWith('open_download_window', {
      initialUrl: '',
      bulk: false,
    })
    await user.click(screen.getByRole('menuitem', { name: 'Pages' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'Settings' })
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(
      invoke.mock.calls.filter(
        ([command]) => command === 'open_download_window',
      ),
    ).toHaveLength(2)
  })
  it('renders initial query failure instead of a router crash', async () => {
    invoke.mockRejectedValue({
      code: 'INTERNAL',
      message: 'unavailable',
      retryable: false,
    })
    await setup('/downloads')
    expect(
      await screen.findByText(
        'Could not load downloads',
        {},
        { timeout: 4000 },
      ),
    ).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: 'Try again' }),
    ).toBeInTheDocument()
    expect(
      screen.queryByText('This page could not be opened'),
    ).not.toBeInTheDocument()
  })
  it('updates the visible table from the shared cache', async () => {
    invoke.mockResolvedValue([task()])
    const { client } = await setup('/downloads')
    await screen.findByRole('table')
    mergeTask(client, task({ status: 'paused', revision: 2 }))
    await waitFor(() =>
      expect(
        within(screen.getByRole('table')).getByText('Paused'),
      ).toBeInTheDocument(),
    )
  })
})

function paste(target: Element, text: string) {
  fireEvent.paste(target, { clipboardData: { getData: () => text } })
}
it('opens single mode for one pasted link and bulk mode with deduplicated links', async () => {
  invoke.mockResolvedValue([])
  await setup('/downloads')
  const main = screen.getByRole('main')
  paste(main, ' https://example.com/one.zip ')
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('open_download_window', {
      initialUrl: 'https://example.com/one.zip',
      bulk: false,
    }),
  )
  paste(
    main,
    'https://example.com/a.zip\r\nhttps://example.com/b.pdf\nhttps://example.com/a.zip',
  )
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('open_download_window', {
      initialUrl: '',
      bulk: true,
      initialUrls: ['https://example.com/a.zip', 'https://example.com/b.pdf'],
    }),
  )
})
it('leaves input and contenteditable pastes alone and ignores unrelated clipboard text', async () => {
  invoke.mockResolvedValue([task()])
  await setup('/downloads')
  const input = await screen.findByRole('textbox', { name: 'Search' })
  paste(input, 'https://example.com/file')
  const editor = document.createElement('div')
  editor.setAttribute('contenteditable', 'true')
  const child = document.createElement('span')
  editor.append(child)
  document.body.append(editor)
  paste(child, 'https://example.com/file')
  editor.remove()
  paste(screen.getByRole('main'), 'ordinary clipboard text')
  expect(
    invoke.mock.calls.some(([command]) => command === 'open_download_window'),
  ).toBe(false)
})
it('does not open a download window when pasted links exceed the import limit', async () => {
  invoke.mockResolvedValue([])
  await setup('/downloads')
  paste(
    screen.getByRole('main'),
    Array.from({ length: 201 }, (_, i) => `https://example.com/${i}`).join(
      '\n',
    ),
  )
  expect(
    invoke.mock.calls.some(([command]) => command === 'open_download_window'),
  ).toBe(false)
})
