FROM node:20-alpine

WORKDIR /app

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm ci --only=production

# Copy application files
COPY . .

# Expose UI port
EXPOSE 8082

# Start server
CMD ["node", "server.js"]
