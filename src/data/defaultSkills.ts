export const defaultSkills = {
  examples: [
    {
      id: 'skill-creator',
      name: 'skill-creator',
      description: 'Create new skills, modify and improve existing skills. Use when users want to create a skill from scratch or edit an existing skill.',
      content: `---\nname: skill-creator\ndescription: Create new skills, modify and improve existing skills. Use when users want to create a skill from scratch or edit an existing skill.\n---\n\n# Skill Creator\n\nYou are the Skill Creator. Help users create, edit, and improve skills (reusable instruction sets).\n\n## When first invoked, greet the user:\n\nWelcome! I'm ready to help you create, improve, or evaluate a skill. Here's what I can help with:\n\n- **Create a new skill** from scratch — I'll guide you through defining what it does, writing the instructions, and testing it\n- **Edit or improve an existing skill** — refine its instructions or optimize how reliably it triggers\n- **Optimize the description** so the skill triggers reliably\n\nWhat are you looking to do? Do you have something specific in mind, or are you starting fresh?\n\n## Creating a skill\n\n### Step 1: Understand intent\n\nAsk the user (one question at a time, conversational tone):\n1. What should this skill enable Claude to do?\n2. When should this skill trigger? (what user phrases/contexts)\n3. What's the expected output format?\n\n### Step 2: Write the SKILL.md\n\nSave to: \`~/.claude/skills/<skill-name>/SKILL.md\`\n\n\`\`\`markdown\n---\nname: skill-name\ndescription: One-line description of what it does and when to use it. Be specific about trigger conditions. Make descriptions slightly "pushy" to avoid under-triggering.\n---\n\nInstructions for the skill go here.\n\`\`\`\n\n### Step 3: Confirm\n\nTell the user:\n- The skill has been created at \`~/.claude/skills/<skill-name>/\`\n- It will be available in the next conversation\n- They can edit or improve it anytime\n\n## Skill Writing Tips\n\n- **Description is critical** — this is the primary trigger mechanism. Include BOTH what the skill does AND specific contexts. Example: instead of "Format data" write "Format tabular data into clean markdown tables. Use whenever the user mentions tables, CSV formatting, data organization, or wants to display structured data."\n- Keep instructions under 200 lines\n- Use imperative form ("Do X", not "You should do X")\n- Explain the WHY behind instructions — models work better with reasoning\n- Include examples of expected input/output when helpful\n\n## Editing an Existing Skill\n\n1. Read the existing \`~/.claude/skills/<skill-name>/SKILL.md\`\n2. Discuss with the user what to change\n3. Make targeted improvements\n4. Save the updated file`,
      is_example: true,
      source_dir: 'skill-creator',
      enabled: true,
      files: [
        { name: 'SKILL.md', type: 'file' },
        {
          name: 'agents', type: 'folder', children: [
            { name: 'analyzer.md', type: 'file' },
            { name: 'comparator.md', type: 'file' },
            { name: 'grader.md', type: 'file' },
          ]
        },
        {
          name: 'assets', type: 'folder', children: [
            { name: 'eval_review.html', type: 'file' },
          ]
        },
        {
          name: 'eval-viewer', type: 'folder', children: [
            { name: 'generate_review.py', type: 'file' },
            { name: 'viewer.html', type: 'file' },
          ]
        },
        {
          name: 'references', type: 'folder', children: [
            { name: 'schemas.md', type: 'file' },
          ]
        },
        {
          name: 'scripts', type: 'folder', children: [
            { name: '__init__.py', type: 'file' },
            { name: 'aggregate_benchmark.py', type: 'file' },
            { name: 'generate_report.py', type: 'file' },
            { name: 'improve_description.py', type: 'file' },
            { name: 'package_skill.py', type: 'file' },
            { name: 'quick_validate.py', type: 'file' },
            { name: 'run_eval.py', type: 'file' },
            { name: 'run_loop.py', type: 'file' },
            { name: 'utils.py', type: 'file' },
          ]
        },
        { name: 'LICENSE.txt', type: 'file' },
      ]
    }
  ],
  my_skills: []
};
