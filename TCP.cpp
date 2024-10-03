#include <iostream>
#include <WinSock2.h>
#include <ws2tcpip.h> // For IPv6-related structures
#define PORT 5000     // The server's port number

#pragma comment(lib, "Ws2_32.lib")

class TCPClient
{
private:
    struct sockaddr_in6 server; // sockaddr_in6 for IPv6
    int clientSocket;
    WSADATA ws;

public:
    TCPClient() : clientSocket(-1) {}

    ~TCPClient()
    {
        closeSocket();
        WSACleanup(); // Clean up Winsock
    }

    bool initializeWinsock()
    {
        int err = WSAStartup(MAKEWORD(2, 2), &ws);
        if (err != 0)
        {
            std::cout << "Failed to initialize Winsock" << std::endl;
            return false;
        }
        std::cout << "Winsock initialized successfully!" << std::endl;
        return true;
    }

    bool createSocket()
    {
        clientSocket = socket(AF_INET6, SOCK_STREAM, IPPROTO_TCP);
        if (clientSocket < 0)
        {
            std::cout << "Failed to create socket." << std::endl;
            return false;
        }

        std::cout << "Socket created successfully." << std::endl;
        return true;
    }

    bool connectToServer(const std::string &serverIP)
    {
        // Define the server address 
        server.sin6_family = AF_INET6;
        server.sin6_port = htons(PORT);

        // Convert server IP from text to binary form
        if (inet_pton(AF_INET6, serverIP.c_str(), &server.sin6_addr) <= 0)
        {
            std::cout << "Invalid server IP address." << std::endl;
            return false;
        }

        // Connect to the server
        int err = connect(clientSocket, (struct sockaddr *)&server, sizeof(server));
        if (err < 0)
        {
            std::cout << "Failed to connect to server." << std::endl;
            return false;
        }

        std::cout << "Connected to server: " << serverIP << std::endl;
        return true;
    }

    void closeSocket()
    {
        if (clientSocket >= 0)
        {
            closesocket(clientSocket);
            std::cout << "Client socket closed." << std::endl;
        }
    }

    void runClient(const std::string &serverIP)
    {
        if (!initializeWinsock()) return;
        if (!createSocket()) return;
        if (!connectToServer(serverIP)) return;

        closeSocket();
    }
};

class TCPServer{
    private:
    struct sockaddr_in6 serverAddr, clientAddr; // IPv6 address structure
    int serverSocket, clientSocket;
    WSADATA ws;

public:
    TCPServer() : serverSocket(-1), clientSocket(-1) {}

    ~TCPServer()
    {
        closeSocket();
        WSACleanup(); // Clean up Winsock
    }

    bool initializeWinsock()
    {
        int err = WSAStartup(MAKEWORD(2, 2), &ws);
        if (err != 0)
        {
            std::cout << "Failed to initialize Winsock" << std::endl;
            return false;
        }
        std::cout << "Winsock initialized successfully!" << std::endl;
        return true;
    }

    bool createSocket()
    {
        serverSocket = socket(AF_INET6, SOCK_STREAM, IPPROTO_TCP);
        if (serverSocket < 0)
        {
            std::cout << "Failed to create socket." << std::endl;
            return false;
        }

        std::cout << "Socket created successfully." << std::endl;
        return true;
    }

    bool bindSocket()
    {
        // Zero out the sockaddr_in6 structure
        memset(&serverAddr, 0, sizeof(serverAddr));

        serverAddr.sin6_family = AF_INET6;
        serverAddr.sin6_port = htons(PORT);
        serverAddr.sin6_addr = in6addr_any; // Listen on any available IPv6 address

        // Bind the socket to the address and port
        if (bind(serverSocket, (struct sockaddr *)&serverAddr, sizeof(serverAddr)) < 0)
        {
            std::cout << "Failed to bind socket." << std::endl;
            return false;
        }

        std::cout << "Socket bound to port " << PORT << "." << std::endl;
        return true;
    }

    bool listenForConnections(int backlog = 5)
    {
        if (listen(serverSocket, backlog) < 0)
        {
            std::cout << "Failed to listen for connections." << std::endl;
            return false;
        }

        std::cout << "Listening for incoming connections..." << std::endl;
        return true;
    }

    bool acceptConnection()
    {
        int clientAddrLen = sizeof(clientAddr);
        clientSocket = accept(serverSocket, (struct sockaddr *)&clientAddr, &clientAddrLen);

        if (clientSocket < 0)
        {
            std::cout << "Failed to accept client connection." << std::endl;
            return false;
        }

        std::cout << "Client connection accepted." << std::endl;
        return true;
    }

    void closeSocket()
    {
        if (clientSocket >= 0)
        {
            closesocket(clientSocket);
            std::cout << "Client socket closed." << std::endl;
        }

        if (serverSocket >= 0)
        {
            closesocket(serverSocket);
            std::cout << "Server socket closed." << std::endl;
        }
    }

    void runServer()
    {
        if (!initializeWinsock()) return;
        if (!createSocket()) return;
        if (!bindSocket()) return;
        if (!listenForConnections()) return;

        std::cout << "Server is ready to accept a connection..." << std::endl;

        if (!acceptConnection()) return;

        closeSocket();
    }
};

int main()
{
    std::string serverIP = "::1"; 

    TCPServer server;

    TCPClient client;

    client.runClient(serverIP);


    return 0;
}