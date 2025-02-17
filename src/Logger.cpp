#include <fstream>
#include <chrono>
#include <ctime>

enum MsgLogType {
    NONE,
    SEND,
    SEND_PASS,
    SEND_FAIL,
    CONN,
    CONN_PASS,
    CONN_FAIL,
    ERR,
    END
};

class Logger {
private:
    std::ofstream logFile;
    static Logger* instance;
    int packets_sent;
    std::string last_message;
    MsgLogType last_status;
    
    Logger() : packets_sent(0) {
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

    std::string get_last_message() {
        return last_message;
    }

    MsgLogType get_last_status() {
        return last_status;
    }

    int get_packets_sent() {
        return packets_sent;
    }

    std::string readLogFile() {
        std::ifstream file("xplane_plugin_log.txt");
        std::string content((std::istreambuf_iterator<char>(file)),
                           std::istreambuf_iterator<char>());
        return content;
    }

    int logTest (int a, int b){
        return a + b; 
    }

    void log(const std::string &message, MsgLogType code = NONE) {
        last_message = message;
        switch (code) {
            case MsgLogType::NONE:
                break;
            case MsgLogType::SEND_PASS:
                packets_sent += 1;
                last_status = code;
                break;
            default:
                last_status = code;
                break;
        }
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


//---------------------- Testing Function ONLY
    int testMesonBuildSystem() {
        return 0; 
    }


};