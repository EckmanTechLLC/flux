# Multi-Session Development Workflow

**Purpose:** Coordinate multiple Claude sessions for development with proper context management and incremental delivery.

---

## Core Concept

**One foundation session** coordinates multiple **focused implementation sessions** that report back for verification before proceeding.

---

## Session Types

### 1. Foundation Session (Coordinator)
- Long-lived session that persists throughout project
- Reviews work from implementation sessions
- Verifies correctness and completeness
- Provides prompts for next sessions
- Maintains project context across phases
- **DOES NOT WRITE CODE**

### 2. Implementation Sessions (Workers)
- Short-lived, focused on specific tasks
- Complete 1-3 related tasks per session
- Report results back to foundation session
- Session notes document what was done (~200-300 lines)
- **MUST follow checklist in CLAUDE.md**

### 3. Planning Sessions (Architects)
- Create ADRs for major decisions
- Break phases into tasks with priorities
- Define schemas, APIs, architectures
- Keep plans concise (~300 lines, not 2000)

---

## Project Structure

```
/docs/
  CLAUDE.md              # Persistent context (read by all sessions)
  /decisions/            # ADRs (001, 002, 003...)
  /sessions/             # Session notes (one per implementation session)
  /workflow/             # This doc
```

---

## Workflow Steps

### Phase Planning
1. Foundation session creates planning session
2. Planning session reads full docs, creates ADR with task breakdown
3. Foundation verifies plan alignment with vision
4. Break into 4-6 implementation sessions

### Implementation Loop
1. **Foundation:** Provides session name and detailed prompt
2. **User:** Starts implementation session with prompt
3. **Implementation:** Completes tasks, updates session notes, updates CLAUDE.md
4. **Implementation:** Reports summary back to user
5. **User:** Pastes summary to foundation session
6. **Foundation:** Verifies work (reads key files)
7. **Foundation:** Provides next session prompt
8. **Repeat** until phase complete

---

## Prompt Template

```
Read /home/etl/projects/flux/CLAUDE.md first.

IMPORTANT:
- Be concise. Session notes ~200-300 lines max.
- Follow the Implementation Session Checklist for EVERY change.

Implement [Phase X Task Y]: [Task Name]

Reference:
- /docs/decisions/[ADR-XXX].md
- /docs/sessions/[previous-task].md

Scope:
[Specific files and what to implement]

Create session note: /docs/sessions/YYYY-MM-DD-[phase]-[task].md
```

---

## Benefits

✓ **Context management:** Foundation maintains big picture, workers focus on tasks
✓ **Quality gates:** Foundation verifies before proceeding
✓ **Incremental delivery:** Small sessions = quick iterations
✓ **Documentation:** Session notes create audit trail
✓ **Recovery:** Can resume at any session boundary
✓ **Conciseness:** Explicit limits prevent over-documentation

---

## Anti-Patterns (Avoid)

✗ Single mega-session trying to do everything
✗ No verification between tasks
✗ Verbose documentation (2000+ line planning docs)
✗ Implementation sessions that start planning from scratch
✗ Skipping CLAUDE.md updates
✗ Vague prompts without specific file paths
✗ Implementation sessions that don't follow the checklist

---

## Key Success Factors

1. **Foundation session longevity:** Keep it alive across entire phase/project
2. **Clear boundaries:** One session = one clear scope
3. **Explicit verification:** Foundation reads actual files, not just summaries
4. **Concise prompts:** Specific files, clear scope, no ambiguity
5. **Regular updates:** Keep CLAUDE.md current after each session
6. **Session notes discipline:** ~200-300 lines, template-based
7. **Checklist adherence:** Every implementation session follows checklist

---

## Session Notes Template

```markdown
# [Phase X Task Y]: [Task Name]

**Date:** YYYY-MM-DD
**Session Type:** Implementation
**Status:** Complete / In Progress / Blocked

---

## Task Summary

[1-2 sentences describing what was accomplished]

---

## Changes Made

### Files Created
- `/path/to/file1.rs` - Purpose
- `/path/to/file2.ts` - Purpose

### Files Modified
- `/path/to/existing.rs` - Changes made
- `/path/to/other.ts` - Changes made

### Lines of Code
- Added: ~XXX lines
- Modified: ~XXX lines

---

## Checklist Completion

- [x] READ FIRST - Read CLAUDE.md, ADRs, existing code
- [x] VERIFY ASSUMPTIONS - Checked files exist, APIs match
- [x] MAKE CHANGES - Small, focused changes
- [x] TEST & VERIFY - Tests pass, no regressions
- [x] DOCUMENT - Session notes, CLAUDE.md updated
- [x] REPORT - Provided summary to user

---

## Test Results

[Test output, verification steps, what was confirmed]

---

## Issues Encountered

[Any problems and how they were resolved]

---

## Next Steps

[What should happen next, blockers, questions]
```
