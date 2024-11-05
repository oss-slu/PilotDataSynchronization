#include <WinSock2.h>
#include <ws2tcpip.h>
#include <iostream>
#include <string>
#include <thread>
#include <chrono>

#define PORT 8089
#pragma comment(lib, "Ws2_32.lib")

class TCPClient {
private:
    struct sockaddr_in server;  // Changed to IPv4
    int clientSocket;
    WSADATA ws;

public:
    TCPClient() : clientSocket(-1) {
        std::cout << "Client starting up..." << std::endl;
    }
    
    ~TCPClient() {
        closeSocket();
        WSACleanup();
        std::cout << "Client shut down." << std::endl;
    }

    bool initializeWinsock() {
        std::cout << "Initializing Winsock..." << std::endl;
        int err = WSAStartup(MAKEWORD(2, 2), &ws);
        if (err != 0) {
            std::cout << "Failed to initialize Winsock. Error code: " << err << std::endl;
            return false;
        }
        std::cout << "Winsock initialized successfully!" << std::endl;
        return true;
    }

    bool createSocket() {
        std::cout << "Creating socket..." << std::endl;
        clientSocket = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);  // Changed to IPv4
        if (clientSocket < 0) {
            std::cout << "Failed to create socket. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }
        std::cout << "Socket created successfully." << std::endl;
        return true;
    }

    bool connectToServer(const std::string& serverIP) {
        std::cout << "Connecting to server at " << serverIP << ":" << PORT << "..." << std::endl;
        server.sin_family = AF_INET;  // Changed to IPv4
        server.sin_port = htons(PORT);
        if (inet_pton(AF_INET, serverIP.c_str(), &server.sin_addr) <= 0) {  // Changed to IPv4
            std::cout << "Invalid server IP address. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }

        int err = connect(clientSocket, (struct sockaddr*)&server, sizeof(server));
        if (err < 0) {
            std::cout << "Failed to connect to server. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }
        std::cout << "Connected to server: " << serverIP << std::endl;
        return true;
    }

    bool sendData(const std::string& message) {
        std::cout << "Sending message: " << message << std::endl;
        int result = send(clientSocket, message.c_str(), message.length(), 0);
        if (result < 0) {
            std::cout << "Failed to send data. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }
        std::cout << "Successfully sent " << result << " bytes." << std::endl;
        return true;
    }

    void closeSocket() {
        if (clientSocket >= 0) {
            closesocket(clientSocket);
            std::cout << "Client socket closed." << std::endl;
        }
    }
};

int main() {
    std::cout << "TCP Client Starting..." << std::endl;
    TCPClient client;
    
    if (!client.initializeWinsock()) {
        std::cout << "Failed to initialize Winsock" << std::endl;
        return 1;
    }
    
    if (!client.createSocket()) {
        std::cout << "Failed to create socket" << std::endl;
        return 1;
    }

    std::string serverIP = "127.0.0.1";  // Changed to IPv4 localhost
    if (!client.connectToServer(serverIP)) {
        std::cout << "Failed to connect to server" << std::endl;
        return 1;
    }
    
    // Send test messages
    std::cout << "Starting to send test messages..." << std::endl;
    for (int i = 0; i < 5; i++) {
        std::string message = "Test message " + std::to_string(i + 1);
        if (!client.sendData(message))
            break;
        std::cout << "Sleeping for 1 second..." << std::endl;
        std::this_thread::sleep_for(std::chrono::seconds(1));
    }
    
    std::cout << "Client shutting down..." << std::endl;
    return 0;
}