import 'vue-router'
declare module 'vue-router' {
  interface RouteMeta {
    /** Permission code required to open the route. */
    perm?: string
    /** Reachable without logging in. */
    public?: boolean
    /** Logged-in users are sent to `/` instead (e.g. the login page). */
    guestOnly?: boolean
    title?: string
  }
}
