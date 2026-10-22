import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app

if (import.meta.env.DEV) {
  // handles for debugging in the console
  Promise.all([import('./lib/stores/route.svelte'), import('./lib/stores/plan.svelte'), import('./lib/stores/app.svelte')]).then(([r, p, a]) => {
    ;(window as any).__fp = { route: r.route, plan: p.plan, app: a.app }
  })
}
