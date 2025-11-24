# Experiments Feature - User Guide

## Quick Start

The Experiments feature lets you tweak and test variations of your LLM requests to optimize prompts, compare models, and debug issues.

## Accessing Experiments

You can access experiments in two ways:

1. **From the Sidebar**: Click the "Experiments" menu item (✨ icon) to view all your experiments
2. **From Traces**: Create new experiments directly from any model invocation span

## How to Create an Experiment

### Step 1: Find a Request to Experiment With

1. Navigate to the **Chat/Traces** page
2. Find a conversation thread you want to experiment with
3. Click on the trace to view its details
4. Locate a **model invocation span** (the actual LLM call)

### Step 2: Open the Experiment Interface

1. In the span details panel, look for the **"Experiment"** button (with sparkle icon ✨)
2. Click the button to open the Experiment page
3. The page will load with your original request pre-populated

### Step 3: Modify Your Request

You can edit your request in two ways:

#### Visual Mode (Recommended for Beginners)
- **Add Messages**: Click "+ Add Message" to add new conversation turns
- **Edit Messages**: Click on any message text box to modify the content
- **Delete Messages**: Click the ✕ button on any message
- **Change Role**: Messages can be system, user, or assistant

#### JSON Mode (Advanced)
- Switch to JSON tab to edit the raw request
- Useful for bulk edits or complex modifications
- Make sure JSON is valid before running

### Step 4: Adjust Parameters

At the bottom of the page, you can modify:

**Basic Parameters:**
- **Model**: Choose from GPT-4, Claude, or other available models
- **Temperature**: Control randomness (0 = focused, 2 = creative)

**Advanced Parameters** (click "Show Advanced"):
- **Max Tokens**: Limit response length
- Additional model-specific parameters

### Step 5: Run Your Experiment

1. Click the **"Run"** button
2. Wait for the request to complete (button shows "Running...")
3. View the results in the right panel

### Step 6: Compare Results

The right panel shows:
- **New Output**: Your experiment's result (green border)
- **Original Output**: The original request's result (gray dashed border)

Compare them to see how your changes affected the response!

## Managing Your Experiments

### Viewing All Experiments

1. Click **"Experiments"** in the sidebar (✨ icon)
2. See all your experiments with:
   - Status indicators (draft, running, completed, failed)
   - Creation time
   - Original span reference
   - Quick actions

### Opening an Experiment

1. From the experiments list, click **"Open"** on any experiment
2. The experiment page loads with the original request data
3. Continue editing and running variations

### Deleting an Experiment

1. Click the trash icon (🗑️) next to any experiment
2. Confirm deletion
3. The experiment is permanently removed

## Common Use Cases

### 1. Prompt Engineering
**Goal**: Improve the quality of responses

**Steps**:
1. Find a response that wasn't quite right
2. Open experiment mode
3. Modify the system message to give clearer instructions
4. Adjust the user message to be more specific
5. Run and compare
6. Iterate until you get better results

**Example**:
```
Original: "You are a helpful assistant."
Improved: "You are a technical writer who explains complex topics in simple terms. Always use examples."
```

### 2. Temperature Testing
**Goal**: Find the right balance between creativity and consistency

**Steps**:
1. Start with your current request
2. Try temperature 0.3 for more focused responses
3. Try temperature 1.5 for more creative responses
4. Compare outputs
5. Pick the temperature that works best

**Temperature Guide**:
- **0.0 - 0.3**: Very focused, deterministic (good for data extraction)
- **0.4 - 0.7**: Balanced (good for general chat)
- **0.8 - 1.5**: Creative, varied (good for brainstorming)
- **1.6 - 2.0**: Very random (experimental)

### 3. Model Comparison
**Goal**: Find the best model for your use case

**Steps**:
1. Start with your current model
2. Change to a different model (e.g., GPT-4 → Claude)
3. Keep everything else the same
4. Run and compare outputs
5. Consider cost, speed, and quality

**Model Selection Guide**:
- **GPT-4**: Best for complex reasoning, expensive
- **GPT-4-Turbo**: Faster and cheaper than GPT-4
- **GPT-3.5**: Fast and cheap, good for simple tasks
- **Claude-3-Opus**: Great for analysis and writing
- **Claude-3-Sonnet**: Balanced speed and quality

### 4. Debugging
**Goal**: Figure out why a request failed or gave wrong results

**Steps**:
1. Find the problematic request
2. Open experiment mode
3. Simplify the request step by step
4. Test each variation
5. Identify what caused the issue

### 5. Context Optimization
**Goal**: Find the minimum context needed for good results

**Steps**:
1. Start with a request that has lots of messages
2. Remove messages one by one
3. Test each variation
4. Find the minimal working context
5. Save tokens and cost!

## Tips and Tricks

### Visual Mode Tips
- **Copy/Paste**: You can copy message content from anywhere
- **Message Order**: Messages are sent in the order shown
- **System Messages**: Put these first for best results
- **Long Messages**: Text areas auto-expand as you type

### JSON Mode Tips
- **Format JSON**: Use Ctrl+Shift+F (Cmd+Shift+F on Mac) to format
- **Validation**: Invalid JSON will show an error when you try to run
- **Copy Output**: Copy the JSON to use in your code

### General Tips
- **Start Small**: Make one change at a time to see its effect
- **Take Notes**: Keep track of what works and what doesn't
- **Check Costs**: Different models have different pricing
- **Save Good Prompts**: Copy prompts that work well

## Keyboard Shortcuts

- **Ctrl/Cmd + Enter**: Run experiment
- **Ctrl/Cmd + B**: Toggle Visual/JSON mode
- **Escape**: Go back to traces

## Understanding Results

### What to Look For

**Quality**:
- Is the response more accurate?
- Does it follow instructions better?
- Is the tone appropriate?

**Consistency**:
- Run the same prompt multiple times
- Higher temperature = more variation
- Lower temperature = more consistent

**Cost**:
- Check usage metrics in the original output
- Compare input/output token counts
- Factor this into your model choice

### When to Iterate

Keep experimenting if:
- The response is still not quite right
- You want to reduce token usage
- You need faster responses
- The cost is too high

Stop experimenting when:
- You get consistent good results
- Further changes don't improve quality
- You've found the right balance of speed/cost/quality

## Saving Your Work

Experiments are automatically saved when you create them from a traced span. You can:

1. **Access Saved Experiments**: Click "Experiments" in the sidebar to see all saved experiments
2. **Reopen Later**: Click "Open" on any experiment to continue working
3. **Export to Code**: Copy the JSON to use the optimized prompt in your application
4. **Share Findings**: Document what worked in your team's documentation

Future updates will add experiment sharing and collaboration features!

## Troubleshooting

### "Failed to load span data"
- Make sure you clicked "Experiment" from a valid model invocation span
- The span needs to have request data
- Try refreshing the page

### "Failed to run experiment"
- Check that you have API credentials configured
- Verify the model name is correct
- Look at the browser console for detailed errors

### "No output shown"
- Make sure you clicked "Run" after making changes
- Check if the request is still running
- Verify the model responded (check network tab)

### Results don't change
- Verify you saved your changes (click outside text boxes)
- Try making more dramatic changes
- Check if temperature is set to 0 (very deterministic)

## Best Practices

1. **Start with Small Changes**: Don't change everything at once
2. **Test Multiple Times**: Run the same prompt 2-3 times to check consistency
3. **Document Your Findings**: Keep notes on what works
4. **Share with Team**: Tell others about successful experiments
5. **Monitor Costs**: Keep an eye on token usage
6. **Use Version Control**: Track prompt changes in your codebase

## Getting Help

If you encounter issues:

1. Check this guide first
2. Look at the detailed documentation: `docs/features/EXPERIMENTS.md`
3. Check the GitHub issues
4. Ask in the community Slack

## What's Next?

Planned features:
- Save and load experiments
- Share experiments with team members
- A/B testing multiple variations
- Automatic prompt optimization suggestions
- Cost comparison across models
- Performance metrics tracking

---

Happy Experimenting! 🚀✨
