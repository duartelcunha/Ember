```markdown
# Ember Development Patterns

> Auto-generated skill from repository analysis

## Overview
This skill teaches the core development patterns and conventions used in the Ember repository, a TypeScript codebase leveraging Rust as its framework. You'll learn how to structure files, write and organize code, follow commit message conventions, and understand the project's testing patterns. This guide is ideal for contributors seeking to maintain consistency and quality in the Ember codebase.

## Coding Conventions

### File Naming
- Use **PascalCase** for all filenames.
  - **Example:** `MyComponent.ts`, `UserService.ts`

### Import Style
- Use **relative imports** for referencing modules within the project.
  - **Example:**
    ```typescript
    import { User } from './User';
    import { formatDate } from '../utils/formatDate';
    ```

### Export Style
- Use **named exports** instead of default exports.
  - **Example:**
    ```typescript
    // Good
    export function fetchData() { ... }
    export const API_URL = '...';

    // Avoid
    // export default function fetchData() { ... }
    ```

### Commit Message Patterns
- Use **conventional commits**.
- Prefix with `chore` for routine tasks.
- Keep commit messages concise (average ~26 characters).
  - **Example:**  
    ```
    chore: update dependencies
    ```

## Workflows

*No automated workflows were detected in this repository. However, the following conventions and commands are recommended for common tasks.*

## Testing Patterns

- **Test File Naming:**  
  Test files follow the pattern `*.test.*`.
  - **Example:** `UserService.test.ts`
- **Testing Framework:**  
  The specific testing framework is unknown, but tests are colocated with source files using the above pattern.
- **Test Example:**
  ```typescript
  // UserService.test.ts
  import { getUser } from './UserService';

  describe('getUser', () => {
    it('returns user data', () => {
      // test implementation
    });
  });
  ```

## Commands
| Command          | Purpose                                  |
|------------------|------------------------------------------|
| /commit-chore    | Create a chore commit with proper format |
| /new-component   | Scaffold a new PascalCase component      |
| /run-tests       | Run all test files (*.test.*)            |
| /format-imports  | Ensure all imports are relative          |
| /export-named    | Check for named exports in a file        |
```
