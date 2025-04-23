RustBucket
---

# TODO

## rb_server
- [ ] implement a `ListenerManager` similar to `SessionManager` that will handle the listeners.

## rb_client

- [ ] Add session interaction prompt.
- [ ] Ability to interact with a session (starts sending the session id along with the command to the server).

## Executing implant commands workflow

1. The server recieves a command from the client (ex. `ls`) which also includes the session id.
2. Server parses command and executes it (determining which command it is is done by the command registry automatically).
3. Execution function of the command should grab the seesion using the session id from the `CommandContext` and add a task to the session.

## rb_implant

Basic implant that perodically checks in with the server, query pending tasks, execute said tasks and send the results back to the server.
