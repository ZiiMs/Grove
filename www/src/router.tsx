import { createBrowserRouter, RouterProvider } from "react-router-dom"
import HomePage from "./routes/HomePage"
import DocsPage from "./routes/DocsPage"

const router = createBrowserRouter([
  {
    path: "/",
    element: <HomePage />,
  },
  {
    path: "/docs",
    element: <DocsPage />,
  },
])

export function AppRouter() {
  return <RouterProvider router={router} />
}
