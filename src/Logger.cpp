#include <fstream>
#include <chrono>
#include <ctime>

class Logger {
private:
    std::ofstream logFile;
    static Logger* instance;
    
    Logger() {
        // Open log file in append mode
        logFile.open("xplane_plugin_log.txt", std::ios::app);
        log("Logger initialized");
    }

public:
    static Logger* getInstance() {
        if (!instance) {
            instance = new Logger();
        }
        return instance;
    }

    void log(const std::string& message) {
        if (logFile.is_open()) {
            auto now = std::chrono::system_clock::now();
            std::time_t time = std::chrono::system_clock::to_time_t(now);
            char timeStr[26];
            ctime_s(timeStr, sizeof(timeStr), &time);
            std::string timeString(timeStr);
            timeString = timeString.substr(0, timeString.length() - 1); // Remove newline

            logFile << "[" << timeString << "] " << message << std::endl;
            logFile.flush(); // Ensure immediate writing
        }
    }

    ~Logger() {
        if (logFile.is_open()) {
            logFile.close();
        }
    }
};
