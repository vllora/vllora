# Experiment Feature

## Overview

The Experiment feature allows users to create variations of traced LLM requests and test different parameters to see how they affect the output. This is useful for prompt engineering, model comparison, and debugging.

## Features

- **Request Variation**: Create modified versions of traced requests
- **Visual & JSON Editing**: Edit requests using a visual interface or raw JSON
- **Side-by-Side Comparison**: Compare original and new outputs
- **Multiple Model Support**: Test with different models (GPT-4, Claude, etc.)
- **Parameter Tuning**: Adjust temperature, max_tokens, and other parameters
- **Message Manipulation**: Add, edit, or remove messages from the conversation
- **Trace Integration**: Access experiments directly from span details

## User Flow

### Creating an Experiment from a Trace

1. **Navigate to a Trace**: Open the chat/traces page and view a model invocation span
2. **Click "Experiment" Button**: In the ModelInvokeUIDetailsDisplay, click the "Experiment" button
3. **Modify Request**:
   - Edit messages in the visual editor
   - Change model parameters (temperature, max_tokens, etc.)
   - Switch between Visual and JSON editing modes
4. **Run Experiment**: Click the "Run" button to execute the modified request
5. **Compare Results**: View the new output alongside the original output

### Viewing All Experiments

1. **Access Experiments Page**: Click "Experiments" in the sidebar navigation
2. **Browse Experiments**: View all your saved experiments with status indicators
3. **Open Experiment**: Click "Open" on any experiment to continue working on it
4. **Delete Experiment**: Click the trash icon to remove an experiment

## Architecture

### Backend

#### Database Schema

**Table: `experiments`**
```sql
CREATE TABLE experiments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    original_span_id TEXT NOT NULL,
    original_trace_id TEXT NOT NULL,
    original_request TEXT NOT NULL,      -- JSON
    modified_request TEXT NOT NULL,      -- JSON
    headers TEXT,                        -- JSON
    prompt_variables TEXT,               -- JSON (Mustache variables)
    model_parameters TEXT,               -- JSON
    result_span_id TEXT,
    result_trace_id TEXT,
    status TEXT NOT NULL DEFAULT 'draft', -- draft, running, completed, failed
    project_id TEXT,
    created_at TEXT,
    updated_at TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
```

#### API Endpoints

- **POST `/experiments`**: Create a new experiment
- **GET `/experiments`**: List all experiments for a project
- **GET `/experiments/{id}`**: Get a specific experiment
- **GET `/experiments/by-span/{span_id}`**: Get experiments for a specific span
- **PUT `/experiments/{id}`**: Update an experiment
- **DELETE `/experiments/{id}`**: Delete an experiment

#### Request/Response Examples

**Create Experiment**
```bash
POST /experiments
Content-Type: application/json

{
  "name": "Temperature Experiment",
  "description": "Testing different temperature values",
  "original_span_id": "span_123",
  "original_trace_id": "trace_456",
  "original_request": {
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello"}],
    "temperature": 0.7
  },
  "modified_request": {
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello"}],
    "temperature": 1.5
  },
  "model_parameters": {
    "temperature": 1.5,
    "max_tokens": 100
  }
}
```

**Response**
```json
{
  "id": "exp_789",
  "name": "Temperature Experiment",
  "status": "draft",
  "created_at": "2025-11-24T15:00:00Z",
  ...
}
```

### Frontend

#### Components

**ExperimentsPage** (`src/pages/experiments.tsx`)
- Lists all experiments for the current project
- Displays experiment status, creation time, and metadata
- Allows opening and deleting experiments

**ExperimentPage** (`src/pages/experiment.tsx`)
- Main experiment interface
- Handles request editing and execution
- Displays results comparison

**ModelInvokeUIDetailsDisplay** (`src/components/chat/traces/.../model-display.tsx`)
- Added "Experiment" button
- Navigates to experiment page with span_id parameter

**AppSidebar** (`src/components/app-sidebar.tsx`)
- Added "Experiments" menu item with Sparkles icon
- Navigates to experiments list page

#### Routes

- `/experiments`: Experiments list page showing all experiments
- `/experiment?span_id={span_id}`: Experiment page with pre-loaded span data

#### State Management

```typescript
interface ExperimentData {
  name: string;
  description: string;
  messages: Message[];
  model: string;
  temperature: number;
  max_tokens?: number;
  headers?: Record<string, string>;
  promptVariables?: Record<string, string>;
}
```

## Usage Examples

### Example 1: Testing Different Temperatures

1. Open a traced model invocation
2. Click "Experiment"
3. Change temperature from 0.7 to 1.5
4. Click "Run"
5. Compare the outputs to see how temperature affects creativity

### Example 2: Prompt Engineering

1. Open a traced request
2. Click "Experiment"
3. Modify the system message to provide different instructions
4. Add or remove user messages
5. Run and compare results

### Example 3: Model Comparison

1. Start with a GPT-4 traced request
2. Click "Experiment"
3. Change model to Claude-3-Opus
4. Run and compare outputs from different models

## Implementation Details

### Backend Files Modified/Created

1. **Migration**: `core/sqlite_migrations/2025-11-24-152000-0001_add_experiments_table/`
   - `up.sql`: Creates experiments table
   - `down.sql`: Drops experiments table

2. **Schema**: `core/src/metadata/schema.rs`
   - Added experiments table definition
   - Added joinable relationship with projects

3. **Models**: `core/src/metadata/models/experiment.rs`
   - `DbExperiment`: Database model
   - `NewDbExperiment`: Insert model
   - `UpdateDbExperiment`: Update model

4. **Service**: `core/src/metadata/services/experiment.rs`
   - `ExperimentServiceImpl`: Business logic for experiments
   - CRUD operations

5. **Handlers**: `gateway/src/handlers/experiments.rs`
   - HTTP request handlers
   - Request/response serialization

6. **Routes**: `gateway/src/http.rs`
   - Added `/experiments` scope with all endpoints

### Frontend Files Modified/Created

1. **List Page**: `ui/src/pages/experiments.tsx`
   - Displays all experiments for a project
   - Shows experiment status, metadata, and actions
   - Allows opening and deleting experiments

2. **Detail Page**: `ui/src/pages/experiment.tsx`
   - Main experiment interface
   - Request editing (visual and JSON modes)
   - Results display and comparison

3. **Component**: `ui/src/components/chat/traces/.../model-display.tsx`
   - Added "Experiment" button
   - Navigation to experiment page

4. **Sidebar**: `ui/src/components/app-sidebar.tsx`
   - Added "Experiments" menu item with Sparkles icon

5. **Router**: `ui/src/App.tsx`
   - Added `/experiments` route (list page)
   - Added `/experiment` route (detail page)

## Future Enhancements

- ✅ **Experiment Persistence**: Save and load experiments (IMPLEMENTED)
- ✅ **Experiments List**: View all experiments in one place (IMPLEMENTED)
- **Batch Experiments**: Run multiple variations in parallel
- **A/B Testing**: Compare multiple model configurations side-by-side
- **Prompt Variables**: Support Mustache template variables
- **Export/Import**: Share experiments with team members
- **Metrics Tracking**: Track cost, latency, and quality metrics across runs
- **Version Control**: Track experiment iterations and history
- **Collaboration**: Share and comment on experiments
- **Favorites**: Mark successful experiments for easy access
- **Tags/Labels**: Organize experiments by category or use case
- **Duplicate**: Clone existing experiments to try variations

## Testing

### Manual Testing Steps

1. **Setup**:
   - Run the backend: `cargo run serve`
   - Run the frontend: `npm run dev`

2. **Test Flow**:
   - Send a chat completion request to create a trace
   - Navigate to traces in the UI
   - Open a model invocation span
   - Click "Experiment" button
   - Verify experiment page loads with original request data
   - Modify messages and parameters
   - Click "Run"
   - Verify new output appears
   - Compare with original output

3. **API Testing**:
```bash
# Create experiment
curl -X POST http://localhost:8080/experiments \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Experiment",
    "original_span_id": "test_span",
    "original_trace_id": "test_trace",
    "original_request": {"model": "gpt-4", "messages": []},
    "modified_request": {"model": "gpt-4", "messages": []}
  }'

# List experiments
curl http://localhost:8080/experiments

# Get by span
curl http://localhost:8080/experiments/by-span/test_span
```

## Troubleshooting

### Common Issues

1. **Experiment button not showing**
   - Ensure you're viewing a model invocation span (not other span types)
   - Check that the span has request data

2. **Failed to load span data**
   - Verify the span_id parameter in URL
   - Check backend logs for span query errors
   - Ensure spans API is working

3. **Experiment execution fails**
   - Check that provider credentials are configured
   - Verify the model name is valid
   - Check backend logs for API errors

4. **Database migration errors**
   - Run migrations: `diesel migration run`
   - Check migration files are in correct location
   - Verify database permissions

## Contributing

When contributing to the experiments feature:

1. Follow existing code patterns
2. Add tests for new functionality
3. Update documentation
4. Consider backward compatibility
5. Review security implications

## License

This feature is part of vLLora and follows the same license.
