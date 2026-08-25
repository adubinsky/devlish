# Lesson 5.2: Notifications And Messages

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach how a program can communicate a result to another person or system.

## Learning Goals

- understand when a workflow sends a message
- connect a decision to a notification

## Vocabulary

- notify
- email
- message
- output

## First Example

Open the files here:
- [02_notifications.dvl](/Users/admin/code/devlish/docs/course/05-real-programs/examples/02_notifications.dvl#L1)
- [02_notification_packet.txt](/Users/admin/code/devlish/docs/course/05-real-programs/examples/02_notification_packet.txt#L1)

## Run It

```bash
./bin/devlish run docs/course/05-real-programs/examples/02_notifications.dvl --debug
```

## Big Idea

Not every output is a number or a route.

Sometimes the result of a program is that it sends:
- an email
- a message
- another service action

## Practice

1. Change the review mailbox in the packet.
2. Run the workflow again.
3. Check the debug output to see the new service arguments.
