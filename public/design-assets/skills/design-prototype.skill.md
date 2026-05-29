---
name: Design Prototype Generator
description: Generate interactive HTML prototypes for web and mobile applications
whenToUse: Use this skill to create high-fidelity prototypes with clickable interactions
allowedTools: []
model: claude-3-sonnet
userInvocable: true
triggers:
  - design
  - prototype
  - ui
od:
  mode: generation
  platform: web
  scenario: ui-prototype
  category: design
  preview:
    reload_strategy: debounce
    debounce_ms: 500
  design_system:
    tokens:
      colors:
        primary: "#8B5CF6"
        secondary: "#3B82F6"
        accent: "#EC4899"
        background: "#FFFFFF"
        text: "#1F2937"
      spacing:
        sm: "8px"
        md: "16px"
        lg: "24px"
        xl: "32px"
  inputs:
    - name: prompt
      type: string
      label: Design Prompt
      description: Describe the UI prototype you want to create
      required: true
    - name: platform
      type: string
      label: Target Platform
      description: Web or Mobile
      required: false
      default: web
    - name: style
      type: string
      label: Design Style
      description: Modern, Minimal, or Bold
      required: false
      default: modern
  outputs:
    format: html
    quality: high
  capabilities_required:
    - html-generation
    - css-styling
    - javascript-interaction
---

## Design Prototype Generator

This skill generates interactive HTML prototypes based on your design description.

### Features

- **Interactive Elements**: Buttons, forms, navigation that actually work
- **Responsive Design**: Works on desktop and mobile
- **Modern UI**: Clean, contemporary design aesthetic
- **Customizable**: Easy to modify colors and styles

### Example Usage

```
Create a SaaS dashboard prototype with:
- Sidebar navigation
- Main content area with charts
- User profile dropdown
- Dark mode toggle
```

### Output

The generated prototype will include:
- Fully interactive HTML/CSS/JavaScript
- Responsive layout
- Smooth animations and transitions
- Clean, production-ready code

---

## Preview Template

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Design Preview</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; }
        .preview-container { min-height: 100vh; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); display: flex; align-items: center; justify-content: center; padding: 20px; }
        .card { background: white; border-radius: 20px; box-shadow: 0 20px 60px rgba(0,0,0,0.3); padding: 40px; max-width: 400px; width: 100%; }
        .card-header { text-align: center; margin-bottom: 30px; }
        .card-header h1 { color: #1F2937; font-size: 24px; margin-bottom: 8px; }
        .card-header p { color: #6B7280; font-size: 14px; }
        .form-group { margin-bottom: 20px; }
        .form-group label { display: block; color: #374151; font-size: 13px; font-weight: 500; margin-bottom: 8px; }
        .form-group input { width: 100%; padding: 12px 16px; border: 2px solid #E5E7EB; border-radius: 10px; font-size: 14px; transition: all 0.2s; }
        .form-group input:focus { outline: none; border-color: #8B5CF6; }
        .btn { width: 100%; padding: 14px; background: linear-gradient(135deg, #8B5CF6 0%, #7C3AED 100%); color: white; border: none; border-radius: 10px; font-size: 15px; font-weight: 600; cursor: pointer; transition: transform 0.2s, box-shadow 0.2s; }
        .btn:hover { transform: translateY(-2px); box-shadow: 0 10px 20px rgba(139, 92, 246, 0.4); }
        .btn:active { transform: translateY(0); }
        .links { display: flex; justify-content: space-between; margin-top: 20px; }
        .links a { color: #6B7280; font-size: 13px; text-decoration: none; }
        .links a:hover { color: #8B5CF6; }
        .feature-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-top: 30px; }
        .feature-item { text-align: center; padding: 16px; background: #F9FAFB; border-radius: 12px; }
        .feature-item svg { width: 24px; height: 24px; margin: 0 auto 8px; fill: #8B5CF6; }
        .feature-item span { display: block; font-size: 11px; color: #6B7280; }
    </style>
</head>
<body>
    <div class="preview-container">
        <div class="card">
            <div class="card-header">
                <h1>Welcome to Claude Design</h1>
                <p>Create beautiful prototypes with AI</p>
            </div>
            <form>
                <div class="form-group">
                    <label>Email</label>
                    <input type="email" placeholder="your@email.com" />
                </div>
                <div class="form-group">
                    <label>Password</label>
                    <input type="password" placeholder="Enter password" />
                </div>
                <button type="submit" class="btn">Get Started</button>
            </form>
            <div class="links">
                <a href="#">Forgot password?</a>
                <a href="#">Create account</a>
            </div>
            <div class="feature-grid">
                <div class="feature-item">
                    <svg viewBox="0 0 24 24"><path d="M12 3L1 9l4 2.18v6L12 21l7-3.82v-6l2-1.09V17h2V9L12 3zm6.82 6L12 12.72 5.18 9 12 5.28 18.82 9zM17 15.99l-5 2.73-5-2.73v-3.72L12 15l5-2.73v3.72z"/></svg>
                    <span>AI Powered</span>
                </div>
                <div class="feature-item">
                    <svg viewBox="0 0 24 24"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm-5 14H7v-2h7v2zm3-4H7v-2h10v2zm0-4H7V7h10v2z"/></svg>
                    <span>Real-time</span>
                </div>
                <div class="feature-item">
                    <svg viewBox="0 0 24 24"><path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm-2 16l-4-4 1.41-1.41L10 14.17l6.59-6.59L18 9l-8 8z"/></svg>
                    <span>Beautiful</span>
                </div>
            </div>
        </div>
    </div>
</body>
</html>
```
