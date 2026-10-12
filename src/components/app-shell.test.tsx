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
  it('shows direct page navigation and a resizable category sidebar', async () => {
    invoke.mockResolvedValue([])
    const { view } = await setup('/downloads/missing')
    expect(
      screen.getByRole('navigation', { name: 'Main navigation' }),
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('menuitem', { name: 'Pages' }),
    ).not.toBeInTheDocument()
    expect(view.container.querySelector('aside')).toBeInTheDocument()
    expect(
      screen.getByRole('separator', { name: 'Resize download sidebar' }),
    ).toBeInTheDocument()
    expect(
      await screen.findByRole('link', { name: 'Downloads' }),
    ).toHaveAttribute('aria-current', 'page')
    expect(screen.getByRole('link', { name: 'Overview' })).not.toHaveAttribute(
      'aria-current',
    )
  })
  it('opens Downloads at startup and navigates directly to other pages', async () => {
    invoke.mockResolvedValue([task()])
    const { router, user } = await setup('/')
    await screen.findByRole('table')
    expect(router.state.location.pathname).toBe('/downloads')
    await user.click(await screen.findByRole('link', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'General' })
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
    await user.click(screen.getByRole('button', { name: 'Appearance' }))
    expect(
      await screen.findByRole('button', { name: 'System' }),
    ).toHaveAttribute('aria-pressed', 'true')
    await user.click(screen.getByRole('button', { name: 'Dark' }))
    await waitFor(() => expect(document.documentElement).toHaveClass('dark'))
    expect(localStorage.getItem('vite-ui-theme')).toBe('dark')
    expect(await screen.findByRole('button', { name: 'Dark' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    await user.keyboard('{Escape}')
    await user.click(await screen.findByRole('link', { name: 'Downloads' }))
    expect(
      await screen.findByRole('heading', { name: 'Downloads' }),
    ).toBeInTheDocument()
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
  })
  it('supports keyboard appearance controls and Escape focus return', async () => {
    invoke.mockResolvedValue([])
    const { user } = await setup()
    const options = screen.getByRole('button', { name: 'Appearance' })
    options.focus()
    await user.keyboard('{Enter}')
    expect(
      await screen.findByRole('button', { name: 'System' }),
    ).toBeInTheDocument()
    await user.keyboard('{Escape}')
    await waitFor(() => expect(options).toHaveFocus())
  })
  it('filters downloads through tabs and keeps the search query', async () => {
    invoke.mockResolvedValue([
      task(),
      task({ id: 'paused', status: 'paused', fileName: 'paused.zip' }),
    ])
    const { user, router } = await setup('/downloads?q=zip')
    await user.click(await screen.findByRole('tab', { name: /Paused/ }))
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({
        q: 'zip',
        status: 'paused',
      }),
    )
    expect(screen.getByRole('tab', { name: /Paused/ })).toHaveAttribute(
      'aria-selected',
      'true',
    )
    expect(
      within(await screen.findByRole('table')).getByText('paused.zip'),
    ).toBeInTheDocument()
    expect(
      within(screen.getByRole('table')).queryByText('file.zip'),
    ).not.toBeInTheDocument()
  })
  it('highlights the selected tab and keeps organization in the sidebar', async () => {
    invoke.mockResolvedValue([task({ status: 'completed' })])
    await setup('/downloads?status=completed')
    expect(screen.getByRole('tab', { name: /Completed/ })).toHaveAttribute(
      'aria-selected',
      'true',
    )
    expect(screen.getByRole('tab', { name: /Active/ })).toBeInTheDocument()
    expect(
      screen.getByRole('navigation', { name: 'Download categories' }),
    ).toHaveTextContent('Organize files')
    expect(screen.getByRole('tab', { name: /All/ })).toHaveAttribute(
      'aria-selected',
      'false',
    )
  })
  it('switches the sidebar to settings sections and shows only the chosen panel', async () => {
    invoke.mockResolvedValue([])
    const { user, router } = await setup('/settings')
    const navigation = screen.getByRole('navigation', {
      name: 'Settings navigation',
    })
    expect(
      screen.queryByRole('navigation', { name: 'Download categories' }),
    ).not.toBeInTheDocument()
    await user.click(
      within(navigation).getByRole('link', { name: 'Transfers' }),
    )
    expect(
      await screen.findByRole('heading', { name: 'Transfers' }),
    ).toBeInTheDocument()
    expect(router.state.location.search).toMatchObject({ section: 'downloads' })
    expect(screen.queryByText('Approved folders')).not.toBeInTheDocument()
    await user.click(within(navigation).getByRole('link', { name: 'Folders' }))
    expect(
      await screen.findByRole('heading', { name: 'Folders' }),
    ).toBeInTheDocument()
    expect(screen.getByText('Approved folders')).toBeInTheDocument()
    expect(
      screen.queryByLabelText('Connections per download'),
    ).not.toBeInTheDocument()
  })
  it('supports keyboard tabs and groups active transfer states', async () => {
    invoke.mockResolvedValue([
      task({ id: 'queued', status: 'queued', fileName: 'waiting.zip' }),
      task({ id: 'running', status: 'downloading', fileName: 'running.zip' }),
      task({ id: 'done', status: 'completed', fileName: 'done.zip' }),
    ])
    const { user, router } = await setup('/downloads')
    const all = await screen.findByRole('tab', { name: /All/ })
    all.focus()
    await user.keyboard('{ArrowRight}')
    await user.keyboard('{Enter}')
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({ status: 'active' }),
    )
    expect(screen.getByRole('tab', { name: /Active/ })).toHaveAttribute(
      'aria-selected',
      'true',
    )
    const panel = await screen.findByRole('tabpanel')
    expect(within(panel).getByText('waiting.zip')).toBeInTheDocument()
    expect(within(panel).getByText('running.zip')).toBeInTheDocument()
    expect(within(panel).queryByText('done.zip')).not.toBeInTheDocument()
  })
  it('opens credentials directly without loading unrelated settings panels', async () => {
    invoke.mockResolvedValue([])
    await setup('/settings?section=credentials')
    expect(
      await screen.findByRole('heading', { name: 'Credentials' }),
    ).toBeInTheDocument()
    expect(screen.getByLabelText('Credential password')).toBeInTheDocument()
    expect(screen.queryByLabelText('Proxy origin')).not.toBeInTheDocument()
    expect(
      screen.queryByRole('navigation', { name: 'Download categories' }),
    ).not.toBeInTheDocument()
    expect(
      invoke.mock.calls.some(
        ([command]) =>
          command === 'get_network_settings' ||
          command === 'get_settings' ||
          command === 'list_destinations',
      ),
    ).toBe(false)
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
    await user.click(await screen.findByRole('link', { name: 'Downloads' }))
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
    await user.click(await screen.findByRole('link', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'General' })
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

it('prepares a dropped browser link from the main workspace', async () => {
  invoke.mockResolvedValue([])
  await setup('/downloads')
  const dataTransfer = {
    types: ['text/uri-list', 'text/plain'],
    getData: (type: string) =>
      type === 'text/uri-list'
        ? 'https://example.com/download.zip'
        : 'Download ZIP',
    dropEffect: 'none',
  }
  fireEvent.dragEnter(screen.getByRole('main'), { dataTransfer })
  expect(screen.getByText('Drop links to add downloads')).toBeInTheDocument()
  fireEvent.drop(screen.getByRole('main'), { dataTransfer })
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('open_download_window', {
      initialUrl: 'https://example.com/download.zip',
      bulk: false,
    }),
  )
  expect(
    screen.queryByText('Drop links to add downloads'),
  ).not.toBeInTheDocument()
})

it('opens all download progress from the toolbar', async () => {
  invoke.mockResolvedValue([])
  const { user } = await setup('/downloads')
  await user.click(
    screen.getByRole('button', { name: 'Show all download progress' }),
  )
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('open_progress_window'),
  )
})
